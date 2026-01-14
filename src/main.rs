use std::default;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use clap::{Arg, Command, Parser, Subcommand, command};

use crate::display::clear_screen;
use crate::probe::{cpuset_with_stats, parse_procfs_cpuinfo, parse_sysfs_cpuinfo, probe_cpuset};
use crate::probe::{read_sysfs_uint, validate_cpuset};

mod cpustat;
mod display;
mod errors;
mod probe;

const SAMPLE_INTERVAL: Duration = Duration::from_millis(1);
const DISPLAY_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Parser)]
#[command(name = "freqprobe")]
#[command(version = "1.0")]
#[command(about = "Probe CPU frequency information", long_about = None)]
struct Cli {
    #[arg(long, help = "Set of comma-delimited CPU IDs to track")]
    cpuset: Option<String>,

    #[command(subcommand)]
    interface: Option<Interface>,
}

#[derive(Subcommand, Default)]
enum Interface {
    Procfs,
    #[default]
    Sysfs,
}

fn main() {
    let args = Cli::parse();
    let interface = args.interface.unwrap_or_default();
    let cpuset = args
        .cpuset
        .map(validate_cpuset)
        .unwrap_or_else(probe_cpuset)
        .expect("couldn't determin cpuset");

    let mut now = SystemTime::now();
    let mut next = now + DISPLAY_INTERVAL;
    match interface {
        Interface::Sysfs => {
            let mut cpu_files = parse_sysfs_cpuinfo(&cpuset)
                .context("could not parse sysfs CPU info")
                .unwrap();
            loop {
                for (path, stats) in &mut cpu_files {
                    let sample = read_sysfs_uint(path);
                    stats.add_sample(sample);
                }

                now = SystemTime::now();
                if now > next {
                    clear_screen();
                    for stats in cpu_files.values() {
                        println!("cpu {}: {:.3}MHz", stats.id, stats.avg_mhz())
                    }
                    next = now + DISPLAY_INTERVAL;
                }
                sleep(SAMPLE_INTERVAL);
            }
        }
        Interface::Procfs => {
            let mut cpu_stats = cpuset_with_stats(&cpuset)
                .context("could not parse cpuset")
                .unwrap();
            loop {
                let cpu_frequencies = parse_procfs_cpuinfo(&cpuset)
                    .context("could not parse sysfs CPU info")
                    .unwrap();
                for (id, sample) in cpu_frequencies {
                    if let Some(entry) = cpu_stats.get_mut(&id) {
                        entry.add_sample(sample);
                    }
                }

                now = SystemTime::now();
                if now > next {
                    clear_screen();
                    for stats in cpu_stats.values() {
                        println!("{stats}");
                    }
                    next = now + DISPLAY_INTERVAL;
                }
                sleep(SAMPLE_INTERVAL);
            }
        }
    }
}
