use std::{
    collections::HashMap,
    ffi::c_void,
    fs::File,
    io::{Error, Write},
    ops::Range,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    panic, slice, thread,
};

use fasthash::RandomState;
use libc::{MAP_FAILED, MAP_HUGE_1GB, MAP_PRIVATE, PROT_READ};

#[derive(Default)]
struct Statistics {
    min: f64,
    sum: f64,
    max: f64,
    count: usize,
}

impl Statistics {
    fn add(&mut self, measurement: f64) {
        self.min = self.min.min(measurement);
        self.max = self.max.max(measurement);
        self.sum += measurement;
        self.count += 1;
    }

    fn complete(self) -> (f64, f64, f64) {
        (self.min, (self.sum / self.count as f64), self.max)
    }

    fn merge_with(&mut self, other: Statistics) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.sum += other.sum;
        self.count += other.count;
    }
}

fn main() {
    let file = File::open("measurements.txt").unwrap();
    let size = file.metadata().unwrap().size() as usize;

    let map_addr = unsafe {
        let addr = libc::mmap(
            std::ptr::null_mut::<c_void>(),
            size,
            PROT_READ,
            MAP_PRIVATE | MAP_HUGE_1GB,
            file.as_raw_fd(),
            0,
        );
        if addr == MAP_FAILED {
            panic!("mmap: {}", Error::last_os_error());
        }
        addr.cast::<u8>()
    };
    let mapped_bytes = unsafe { slice::from_raw_parts(map_addr, size) };
    let string = unsafe { str::from_utf8_unchecked(mapped_bytes) };

    let chunk_count: usize = thread::available_parallelism().unwrap().get();
    let chunk_size = size / chunk_count;

    let mut chunks: Vec<Range<usize>> = Vec::new();
    let mut start = 0;
    for _ in 0..chunk_count {
        let exact_end = string.ceil_char_boundary(start + chunk_size);
        let end = string[exact_end..]
            .find('\n')
            .map(|e| e + exact_end)
            // it is always possible that exact_end hits the very last line
            .unwrap_or(size);
        chunks.push(start..end);
        start = end + 1;
    }
    // last chunk gets the remainder of the work
    chunks.last_mut().unwrap().end = size;

    let cities = thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(|| {
                    let s = RandomState::new();
                    let mut cities: HashMap<&str, Statistics, RandomState<fasthash::city::Hash64>> =
                        HashMap::with_capacity_and_hasher(400, s);
                    for line in string[chunk].lines() {
                        let (city, temp) = line.split_once(";").expect(line);

                        let measurement: f64 = temp.parse().unwrap();
                        cities.entry(city).or_default().add(measurement);
                    }
                    cities
                })
            })
            .collect();

        let mut merged: Option<HashMap<&str, Statistics, RandomState<fasthash::city::Hash64>>> =
            None;
        for h in handles {
            let chunk_result = h.join().unwrap();
            if let Some(merged) = merged.as_mut() {
                for (k, v) in chunk_result {
                    merged.entry(k).or_default().merge_with(v);
                }
            } else {
                merged = Some(chunk_result)
            }
        }
        merged.unwrap()
    });

    write!(std::io::stdout(), "{{").unwrap();
    cities.into_iter().enumerate().for_each(|(i, (city, s))| {
        let (min, avg, max) = s.complete();
        write!(
            std::io::stdout(),
            "{}\"{city}\": {{\"min\": {min:.1}, \"avg\": {avg:.1}, \"max\": {max:.1}}}",
            if i != 0 { "," } else { "" }
        )
        .unwrap();
    });
    write!(std::io::stdout(), "}}").unwrap();
    std::io::stdout().flush().unwrap();
}
