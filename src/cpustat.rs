use std::collections::VecDeque;

use std::fmt::Display;

#[derive(Debug)]
pub struct CpuStat {
    pub id: usize,
    pub window_size: usize,
    frequency_samples: VecDeque<u64>,
    sum: u64,
}

impl Display for CpuStat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cpu {}: {:.3}MHz", self.id, self.avg_mhz())
    }
}

impl CpuStat {
    pub fn new(id: usize, window_size: usize) -> Self {
        Self {
            id,
            window_size,
            frequency_samples: VecDeque::with_capacity(window_size),
            sum: 0,
        }
    }

    pub fn avg_mhz(&self) -> f64 {
        self.mean() / 1_000_000.0
    }

    pub fn mean(&self) -> f64 {
        self.sum as f64 / self.frequency_samples.len() as f64
    }

    pub fn add_sample(&mut self, sample: u64) {
        if self.frequency_samples.len() == self.window_size {
            if let Some(v) = self.frequency_samples.pop_front() {
                self.sum -= v;
            }
        }
        self.sum += sample;
        self.frequency_samples.push_back(sample);
    }
}
