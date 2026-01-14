use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufRead, Read};
use std::path::Path;
use std::{fs::File, io::BufReader, path::PathBuf, process::exit};

use crate::errors::ProbeError;

const SYSFS_CPUS: &str = "/sys/devices/system/cpu";
const SYSFS_CPUFREQ: &str = "/sys/devices/system/cpu/cpufreq";
const PROCFS_CPUINFO: &str = "/proc/cpuinfo";

const WINDOW_SIZE: usize = 10000;

use crate::cpustat::CpuStat;

pub fn read_sysfs_uint(path: impl AsRef<Path>) -> u64 {
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

pub fn validate_cpuset(cpuset: String) -> Result<HashSet<usize>, ProbeError> {
    let mut cpu_ids = HashSet::new();
    for id in cpuset.split(",") {
        let id = id
            .parse::<usize>()
            .map_err(|_| ProbeError::InvalidCpuId(id.to_string()))?;
        cpu_ids.insert(id);
    }
    Ok(cpu_ids)
}

pub fn probe_cpuset() -> Result<HashSet<usize>, ProbeError> {
    let entries = fs::read_dir(SYSFS_CPUS).unwrap();
    let mut cpu_ids = HashSet::new();
    for entry in entries {
        if let Ok(ref e) = entry {
            let filename = e.file_name();
            let filename = filename.to_string_lossy();
            if let Some(stripped) = filename.strip_prefix("cpu") {
                // only keep parsing if the cpu has an ID (not cpufreq or other)
                let Ok(id) = stripped.parse::<usize>() else {
                    continue;
                };
                cpu_ids.insert(id);
            }
        } else {
            eprintln!("unable to access entry: {entry:#?}");
        }
    }
    Ok(cpu_ids)
}

fn sysfs_cpu_path(id: usize) -> PathBuf {
    Path::new(SYSFS_CPUS)
        .join(format!("cpu{id}"))
        .join("cpufreq")
        .join("scaling_cur_freq")
}

pub fn cpuset_with_stats(cpuset: &HashSet<usize>) -> Result<BTreeMap<usize, CpuStat>, ProbeError> {
    let cpu_files: BTreeMap<_, _> = cpuset
        .into_iter()
        .map(|&id| (id, CpuStat::new(id, WINDOW_SIZE)))
        .collect();
    Ok(cpu_files)
}

pub fn parse_sysfs_cpuinfo(
    cpuset: &HashSet<usize>,
) -> Result<BTreeMap<PathBuf, CpuStat>, ProbeError> {
    let cpu_files: BTreeMap<_, _> = cpuset
        .into_iter()
        .map(|&id| (sysfs_cpu_path(id), CpuStat::new(id, WINDOW_SIZE)))
        .collect();
    Ok(cpu_files)
}

/// parse /proc/cpuinfo to get every CPU's current frequency
pub fn parse_procfs_cpuinfo(cpuset: &HashSet<usize>) -> Result<BTreeMap<usize, u64>, ProbeError> {
    let mut cpu_frequencies = BTreeMap::new();
    let file = File::open(PROCFS_CPUINFO).expect("couldn't open procfs file");
    let reader = BufReader::new(file);
    let mut current = None;
    for line in reader.lines().map_while(Result::ok) {
        if let Some(id) = current {
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
            cpu_frequencies.insert(id, (frequency_mhz * 1000.0) as u64);
            current = None;
        } else {
            let Some(line) = line.strip_prefix("processor") else {
                continue;
            };
            let line = line.trim_start();
            let Some(line) = line.strip_prefix(":") else {
                eprintln!("incorrrect file format");
                exit(1)
            };
            let line = line.trim_start();
            let id = line.parse::<usize>().expect("couldn't parse frequency");
            if cpuset.contains(&id) {
                current = Some(id);
            }
        }
    }
    Ok(cpu_frequencies)
}
