//
// monitor.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

#[macro_use]
extern crate structopt_derive;
extern crate structopt;
extern crate clap;
extern crate icecc;
extern crate libc;

use structopt::StructOpt;


#[derive(StructOpt)]
struct Options {
    #[structopt(short="n", long="netname", help="Name of the IceCC network")]
    netname: Option<String>,
}


fn discover_scheduler(netname: Option<String>) -> Result<icecc::MessageChannel, &'static str>
{
    let mut disco = icecc::ScheduleDiscoverer::new(netname.as_ref());
    loop {
        match disco.try_get_scheduler() {
            Some(chan) =>
                return Ok(chan),
            None => {
                if disco.timed_out() {
                    return Err("Timed out searching for scheduler");
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}


fn main() {
    let opts = Options::from_args();
    let mut chan = match discover_scheduler(opts.netname) {
        Ok(chan) => chan,
        Err(ref e) => {
            println!("Error: {}", e);
            ::std::process::exit(1);
        }
    };

    chan.bulk_transfer();
    chan.send(icecc::Message::from(icecc::msg::Ping::new()));

    while !chan.eof() {
        let mut pfd = libc::pollfd {
            fd: chan.fd(),
            events: libc::POLLIN,
            revents: 0
        };

        let ret = unsafe {
            libc::poll((&mut [pfd]).as_mut_ptr(), 1, -1)
        };

        while !chan.read_a_bit() || chan.has_message() {
            match chan.recv(None) {
                Some(icecc::Message::MonitorStats(ref stats)) =>
                    handle_monitor_stats(stats),
                Some(icecc::Message::MonitorLocalJobBegin(ref job)) =>
                    handle_monitor_local_job_begin(job),
                Some(icecc::Message::MonitorJobDone(ref job)) =>
                    handle_monitor_job_done(job),
                Some(ref msg) =>
                    println!("Unhandled: {:?}", msg),
                None => (),  // TODO: Handle re-checking scheduler.
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}


fn handle_monitor_stats(msg: &icecc::msg::MonitorStats) {
    println!("stats {}: {}", msg.host_id(), msg.message());
}

fn handle_monitor_local_job_begin(msg: &icecc::msg::MonitorLocalJobBegin) {
    println!("begin {} (local): {}", msg.job_id(), msg.filename());
}

fn handle_monitor_job_done(msg: &icecc::msg::MonitorJobDone) {
    println!("done {} (local)", msg.job_id());
}
