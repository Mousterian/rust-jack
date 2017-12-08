//! Takes 2 audio inputs and outputs them to 2 audio outputs.
//! All JACK notifications are also printed out.
extern crate jack;
use jack::prelude as j;
use std::io;
use std::io::Read;
use std::sync::Arc;
use std::sync::Mutex;
use std::iter::FromIterator;

struct Notifications;

impl j::NotificationHandler for Notifications {
    fn thread_init(&self, _: &j::Client) {
        println!("JACK: thread init");
    }

    fn shutdown(&mut self, status: j::ClientStatus, reason: &str) {
        println!(
            "JACK: shutdown with status {:?} because \"{}\"",
            status,
            reason
        );
    }

    fn freewheel(&mut self, _: &j::Client, is_enabled: bool) {
        println!(
            "JACK: freewheel mode is {}",
            if is_enabled { "on" } else { "off" }
        );
    }

    fn buffer_size(&mut self, _: &j::Client, sz: j::JackFrames) -> j::JackControl {
        println!("JACK: buffer size changed to {}", sz);
        j::JackControl::Continue
    }

    fn sample_rate(&mut self, _: &j::Client, srate: j::JackFrames) -> j::JackControl {
        println!("JACK: sample rate changed to {}", srate);
        j::JackControl::Continue
    }

    fn client_registration(&mut self, _: &j::Client, name: &str, is_reg: bool) {
        println!(
            "JACK: {} client with name \"{}\"",
            if is_reg { "registered" } else { "unregistered" },
            name
        );
    }

    fn port_registration(&mut self, _: &j::Client, port_id: j::JackPortId, is_reg: bool) {
        println!(
            "JACK: {} port with id {}",
            if is_reg { "registered" } else { "unregistered" },
            port_id
        );
    }

    fn port_rename(
        &mut self,
        _: &j::Client,
        port_id: j::JackPortId,
        old_name: &str,
        new_name: &str,
    ) -> j::JackControl {
        println!(
            "JACK: port with id {} renamed from {} to {}",
            port_id,
            old_name,
            new_name
        );
        j::JackControl::Continue
    }

    fn ports_connected(
        &mut self,
        _: &j::Client,
        port_id_a: j::JackPortId,
        port_id_b: j::JackPortId,
        are_connected: bool,
    ) {
        println!(
            "JACK: ports with id {} and {} are {}",
            port_id_a,
            port_id_b,
            if are_connected {
                "connected"
            } else {
                "disconnected"
            }
        );
    }

    fn graph_reorder(&mut self, _: &j::Client) -> j::JackControl {
        println!("JACK: graph reordered");
        j::JackControl::Continue
    }

    fn xrun(&mut self, _: &j::Client) -> j::JackControl {
        println!("JACK: xrun occurred");
        j::JackControl::Continue
    }

    fn latency(&mut self, _: &j::Client, mode: j::LatencyType) {
        println!(
            "JACK: {} latency has changed",
            match mode {
                j::LatencyType::Capture => "capture",
                j::LatencyType::Playback => "playback",
            }
        );
    }
}

fn read_char() -> Option<u8> {
//    let mut user_input = String::new();
    let mut user_input : [u8; 1] = [0];
    match io::stdin().read_exact(&mut user_input) {
        Ok(_) => Some(user_input[0]),
        Err(_) => None,
    }
}

#[derive(Debug,PartialEq)]
enum Transport {
    NotStarted,
    RECORDING,
    LOOPING,
//    OVERDUBBING,
    STOPPING
}

fn main() {
    // Create client
    let (client, _status) = j::Client::new("rust_jack_simple", j::client_options::NO_START_SERVER)
        .unwrap();

    // Register ports. They will be used in a callback that will be
    // called when new data is available.
    let in_a = client
        .register_port("rust_in_l", j::AudioInSpec::default())
        .unwrap();
    let in_b = client
        .register_port("rust_in_r", j::AudioInSpec::default())
        .unwrap();
    let mut out_a = client
        .register_port("rust_out_l", j::AudioOutSpec::default())
        .unwrap();
    let mut out_b = client
        .register_port("rust_out_r", j::AudioOutSpec::default())
        .unwrap();

    let mut loop_buffer_a = Vec::new();
    let mut loop_buffer_b = Vec::new();
    let recording_flag_reader = Arc::new(Mutex::new(Transport::NotStarted));
    let recording_flag_writer = recording_flag_reader.clone();
    let mut time = 0;

    let process_callback = move |_: &j::Client, ps: &j::ProcessScope| -> j::JackControl {
        let mut out_a_p = j::AudioOutPort::new(&mut out_a, ps);
        let mut out_b_p = j::AudioOutPort::new(&mut out_b, ps);
        let in_a_p = j::AudioInPort::new(&in_a, ps);
        let in_b_p = j::AudioInPort::new(&in_b, ps);

        let transport_state_ptr = recording_flag_reader.lock().unwrap();

        match *transport_state_ptr {
            Transport::NotStarted => {
                out_a_p.clone_from_slice(&in_a_p);
                out_b_p.clone_from_slice(&in_b_p);
            }
            Transport::RECORDING => {
                loop_buffer_a.extend(in_a_p.iter().cloned());
                loop_buffer_b.extend(in_b_p.iter().cloned());
            },
            Transport::LOOPING => {
                let frames_wanted = out_a_p.len();
                let frames_to_end_loop = loop_buffer_a.len() - time;
                if frames_to_end_loop >= frames_wanted {
                    let part_vec_a = Vec::from_iter(loop_buffer_a[time..time+frames_wanted].iter().cloned());
                    out_a_p.clone_from_slice(part_vec_a.as_slice());
                    let part_vec_b = Vec::from_iter(loop_buffer_b[time..time+frames_wanted].iter().cloned());
                    out_b_p.clone_from_slice(part_vec_b.as_slice());
                }
                else {
                    let part_vec_a = Vec::from_iter(loop_buffer_a[time..time+frames_to_end_loop].iter().cloned());
                    for i in 0..part_vec_a.len() {
                        out_a_p[i] = part_vec_a[i];
                    }
                    let part_vec_b = Vec::from_iter(loop_buffer_b[time..time+frames_to_end_loop].iter().cloned());
                    for i in 0..part_vec_b.len() {
                        out_b_p[i] = part_vec_b[i];
                    }
                    let remainder_part_vec_a = Vec::from_iter(loop_buffer_a[0..frames_wanted - frames_to_end_loop].iter().cloned());
                    for i in 0..remainder_part_vec_a.len() {
                        out_a_p[frames_to_end_loop + i] = remainder_part_vec_a[i];
                    }
                    let remainder_part_vec_b = Vec::from_iter(loop_buffer_b[0..frames_wanted - frames_to_end_loop].iter().cloned());
                    for i in 0..remainder_part_vec_b.len() {
                        out_b_p[frames_to_end_loop + i] = remainder_part_vec_b[i];
                    }

                }
                time = (time + frames_wanted) % loop_buffer_a.len();
            },
            _ => {},
        }

        j::JackControl::Continue
    };
    let process = j::ClosureProcessHandler::new(process_callback);

    // Activate the client, which starts the processing.
    let active_client = j::AsyncClient::new(client, Notifications, process).unwrap();


    println!("Press any key to start/stop recording... ctrl+c to exit");
    while let Some(_) = read_char() {
        let mut transport_state_ptr = recording_flag_writer.lock().unwrap();
        let new_state = match *transport_state_ptr {
            Transport::NotStarted => Transport::RECORDING,
            Transport::RECORDING => Transport::LOOPING,
            _ => Transport::STOPPING,
        };
        println!("transport state changing from {:?} to {:?}", *transport_state_ptr, new_state);
        let breaking = new_state == Transport::STOPPING;
        *transport_state_ptr = new_state;
        if breaking {
            break;
        }
    }

    println!("goodbye!");

    // Wait for user input to quit
//    println!("Press enter/return to quit...");
//    let mut user_input = String::new();
//    io::stdin().read_line(&mut user_input).ok();

    active_client.deactivate().unwrap();
}
