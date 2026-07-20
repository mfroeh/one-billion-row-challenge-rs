use std::{
    collections::HashMap,
    ffi::c_void,
    fs::File,
    io::{Error, Write},
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    panic, slice,
};

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

    let mut cities: HashMap<&str, Statistics> = HashMap::new();
    for line in string.lines() {
        let (city, temp) = line.split_once(";").unwrap();
        let measurement: f64 = temp.parse().unwrap();
        cities.entry(city).or_default().add(measurement);
    }

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
