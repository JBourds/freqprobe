use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum, command};

use crate::cpustat::CpuStat;
use crate::display::clear_screen;
use crate::probe::{cpuset_with_stats, parse_procfs_cpuinfo, parse_sysfs_cpuinfo, probe_cpuset};
use crate::probe::{read_sysfs_uint, validate_cpuset};

mod cpustat;
mod display;
mod errors;
mod probe;

const DEFAULT_MONITOR_FREQUENCY: u64 = 100;
const DEFAULT_SAMPLE_FREQUENCY: u64 = 1;
const DEFAULT_WINDOW_SIZE: usize = 10_000;
const KILO: u64 = 1_000;

#[derive(Parser)]
#[command(name = "freqprobe")]
#[command(version = "1.0")]
#[command(about = "Probe CPU frequency information", long_about = None)]
struct Cli {
    /// Which interface to use for extracting CPU information (procfs/sysfs).
    /// If left blank, will try to automatically determine the correct choice.
    #[arg(global = true, value_enum, long)]
    interface: Option<Interface>,

    /// Set of CPU IDs to track.
    /// If left blank, will track all detected CPUs.
    #[arg(global = true, long)]
    cpuset: Option<String>,

    /// Number of milliseconds between each data collection point.
    #[arg(global = true, long)]
    sample_freq: Option<u64>,

    /// The format data is output (monitor/file).
    #[command(subcommand)]
    output: Output,
}

#[derive(ValueEnum, Clone, Default)]
enum Interface {
    Procfs,
    #[default]
    Sysfs,
}

#[derive(Subcommand)]
enum Output {
    Monitor {
        /// The frequency which the monitor is refreshed with updated running
        /// averages of `window-size` size.
        #[arg(long)]
        update_freq: Option<u64>,

        /// The number of data points to keep within a running total for
        /// calculating CPU running average frequency.
        #[arg(long)]
        window_size: Option<usize>,
    },
    Log {
        /// Destination CSV file to store CPU data.
        file: PathBuf,
        /// Duration in milliseconds to monitor for before exiting.
        duration_ms: u64,
    },
}

impl Default for Output {
    fn default() -> Self {
        Self::Monitor {
            update_freq: Some(DEFAULT_MONITOR_FREQUENCY),
            window_size: Some(DEFAULT_WINDOW_SIZE),
        }
    }
}

struct Runner {
    interface: Interface,
    cpuset: HashSet<usize>,
    sample_interval: Duration,
    output: Output,
}

impl Runner {
    fn new(
        interface: Interface,
        cpuset: HashSet<usize>,
        sample_frequency_ms: u64,
        output: Output,
    ) -> Self {
        Self {
            interface,
            cpuset,
            sample_interval: Duration::from_millis(sample_frequency_ms),
            output,
        }
    }

    fn run(&mut self) {
        match &self.output {
            Output::Monitor {
                update_freq,
                window_size,
            } => self.monitor(
                update_freq.unwrap_or(DEFAULT_MONITOR_FREQUENCY),
                window_size.unwrap_or(DEFAULT_WINDOW_SIZE),
            ),
            Output::Log { file, duration_ms } => self.log(file.clone(), *duration_ms),
        }
    }
    fn monitor(&mut self, update_frequency_ms: u64, window_size: usize) {
        let mut now = SystemTime::now();
        let update_interval = Duration::from_millis(update_frequency_ms);
        let mut next = now + update_interval;
        match self.interface {
            Interface::Sysfs => {
                let mut cpu_files = parse_sysfs_cpuinfo(&self.cpuset)
                    .context("could not parse sysfs CPU info")
                    .unwrap();
                let mut cpu_stats: BTreeMap<usize, CpuStat> = cpu_files
                    .keys()
                    .map(|&id| (id, CpuStat::new(id, window_size)))
                    .collect();
                loop {
                    for (id, path) in &mut cpu_files {
                        let sample = read_sysfs_uint(path) * KILO;
                        if let Some(stats) = cpu_stats.get_mut(id) {
                            stats.add_sample(sample);
                        }
                    }

                    now = SystemTime::now();
                    if now > next {
                        next = now + update_interval;
                        clear_screen();
                        for stats in cpu_stats.values() {
                            println!("cpu {}: {:.3}MHz", stats.id, stats.avg_mhz())
                        }
                    }
                    sleep(self.sample_interval);
                }
            }
            Interface::Procfs => {
                let mut cpu_stats = cpuset_with_stats(&self.cpuset)
                    .context("could not parse cpuset")
                    .unwrap();
                loop {
                    let cpu_frequencies = parse_procfs_cpuinfo(&self.cpuset)
                        .context("could not parse sysfs CPU info")
                        .unwrap();
                    for (id, sample) in cpu_frequencies {
                        if let Some(entry) = cpu_stats.get_mut(&id) {
                            entry.add_sample(sample);
                        }
                    }

                    now = SystemTime::now();
                    if now > next {
                        next = now + update_interval;
                        clear_screen();
                        for stats in cpu_stats.values() {
                            println!("{stats}");
                        }
                    }
                    sleep(self.sample_interval);
                }
            }
        }
    }

    fn get_log_header(cpuset: &HashSet<usize>) -> Vec<String> {
        let mut header = Vec::with_capacity(cpuset.len());
        header.extend({
            let mut v: Vec<_> = cpuset.iter().collect();
            v.sort();
            v.into_iter().map(|id| format!("cpu{id}"))
        });
        header
    }

    fn log(&mut self, file: impl AsRef<Path>, duration_ms: u64) {
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file)
            .expect("unable to open provided file path for data logging.");

        use csv::Writer;
        let mut writer = Writer::from_writer(file);
        let header = Self::get_log_header(&self.cpuset);
        writer
            .write_record(header)
            .expect("failed to write csv header");
        let end = SystemTime::now() + Duration::from_millis(duration_ms);
        match self.interface {
            Interface::Sysfs => {
                let cpu_files = parse_sysfs_cpuinfo(&self.cpuset)
                    .context("could not parse sysfs CPU info")
                    .unwrap();
                let mut record = Vec::with_capacity(cpu_files.len());
                while SystemTime::now() < end {
                    for path in &mut cpu_files.values() {
                        let sample = read_sysfs_uint(path);
                        record.push(sample.to_string());
                    }
                    writer
                        .write_record(&record)
                        .expect("failed to write csv record");
                    record.clear();
                    sleep(self.sample_interval);
                }
            }
            Interface::Procfs => {
                while SystemTime::now() < end {
                    let cpu_frequencies = parse_procfs_cpuinfo(&self.cpuset)
                        .context("could not parse sysfs CPU info")
                        .unwrap();
                    writer
                        .write_record(cpu_frequencies.into_values().map(|v| v.to_string()))
                        .expect("failed to write csv record");
                    sleep(self.sample_interval);
                }
            }
        }
    }
}

fn main() {
    let args = Cli::parse();
    let interface = args.interface.unwrap_or_default();
    let cpuset = args
        .cpuset
        .map(validate_cpuset)
        .unwrap_or_else(probe_cpuset)
        .expect("couldn't determin cpuset");
    let sample_frequency_ms = args.sample_freq.unwrap_or(DEFAULT_SAMPLE_FREQUENCY);
    let mut runner = Runner::new(interface, cpuset, sample_frequency_ms, args.output);
    runner.run();
}
