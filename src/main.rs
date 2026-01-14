use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::io::{Seek, stdout};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

use clap::Arg;

const SYSFS_CPUS: &str = "/sys/devices/system/cpu";
const SYSFS_CPUFREQ: &str = "/sys/devices/system/cpu/cpufreq";
const PROCFS_CPUINFO: &str = "/proc/cpuinfo";
const WINDOW_SIZE: usize = 10000;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(1);
const DISPLAY_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
struct CpuStat {
    id: usize,
    window_size: usize,
    frequency_samples: VecDeque<u64>,
    sum: u64,
}

impl CpuStat {
    fn new(id: usize, window_size: usize) -> Self {
        Self {
            id,
            window_size,
            frequency_samples: VecDeque::with_capacity(window_size),
            sum: 0,
        }
    }

    fn avg_mhz(&self) -> f64 {
        self.mean() / 1_000_000.0
    }

    fn mean(&self) -> f64 {
        self.sum as f64 / self.frequency_samples.len() as f64
    }

    fn add_sample(&mut self, sample: u64) {
        if self.frequency_samples.len() == self.window_size {
            if let Some(v) = self.frequency_samples.pop_front() {
                self.sum -= v;
            }
        }
        self.sum += sample;
        self.frequency_samples.push_back(sample);
    }
}

fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
    stdout().flush().unwrap();
}

fn read_sysfs_uint(path: impl AsRef<Path>) -> u64 {
    let mut s = String::new();
    let mut file = fs::OpenOptions::new()
        .read(true)
        .open(path)
        .expect("couldn't open file");
    let _ = file
        .read_to_string(&mut s)
        .expect("couldn't read from file");
    let s = s.split_whitespace().take(1).next().unwrap();
    s.parse::<u64>().unwrap()
}

fn get_cpu_files() -> Vec<(PathBuf, CpuStat)> {
    let entries = fs::read_dir(SYSFS_CPUS).unwrap();
    let mut cpu_files = Vec::new();
    for entry in entries {
        if let Ok(ref e) = entry {
            let filename = e.file_name();
            let filename = filename.to_string_lossy();
            if let Some(stripped) = filename.strip_prefix("cpu") {
                // only keep parsing if the cpu has an ID (not cpufreq or other)
                let Ok(id) = stripped.parse::<usize>() else {
                    continue;
                };
                let path = e.path().join("cpufreq").join("scaling_cur_freq");
                cpu_files.push((path, CpuStat::new(id, WINDOW_SIZE)));
            }
        } else {
            eprintln!("unable to access entry: {entry:#?}");
        }
    }
    cpu_files.sort_by_key(|(_, stats)| stats.id);
    cpu_files
}

/// parse /proc/cpuinfo to get every CPU's current frequency
fn parse_procfs_cpuinfo() -> Vec<u64> {
    let mut cpu_frequencies = Vec::new();
    let file = File::open(PROCFS_CPUINFO).expect("couldn't open procfs file");
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        let Some(line) = line.strip_prefix("cpu MHz") else {
            continue;
        };
        let line = line.trim_start();
        let Some(line) = line.strip_prefix(":") else {
            eprintln!("incorrrect file format");
            exit(1)
        };
        let line = line.trim_start();
        let frequency_mhz = line.parse::<f64>().expect("couldn't parse frequency");
        cpu_frequencies.push((frequency_mhz * 1000.0) as u64);
    }
    cpu_frequencies
}

fn main() {
    let mut cpu_files = get_cpu_files();
    let mut now = SystemTime::now();
    let mut next = now + DISPLAY_INTERVAL;
    loop {
        for (path, stats) in &mut cpu_files {
            let sample = read_sysfs_uint(path);
            stats.add_sample(sample);
        }

        now = SystemTime::now();
        if now > next {
            clear_screen();
            for (_, stats) in &cpu_files {
                println!("cpu {}: {:.3}MHz", stats.id, stats.avg_mhz())
            }
            next = now + DISPLAY_INTERVAL;
        }
        sleep(SAMPLE_INTERVAL);
    }
}
