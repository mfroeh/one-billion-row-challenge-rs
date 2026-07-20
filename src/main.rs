use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    mem::ManuallyDrop,
    os::unix::fs::MetadataExt,
};

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
    let mut file = File::open("measurements.txt").unwrap();
    let size = file.metadata().unwrap().size() as usize;
    let mut buf = vec![0; size];

    let mut read = 0;
    while read < size {
        read += file.read(&mut buf[read..]).unwrap();
    }
    let file = unsafe { String::from_raw_parts(buf.as_mut_ptr(), size, buf.capacity()) };
    let _ = ManuallyDrop::new(buf);

    let mut cities: HashMap<&str, Statistics> = HashMap::new();
    for line in file.lines() {
        let (city, temp) = line.split_once(";").unwrap();
        let measurement: f64 = temp.parse().unwrap();
        cities.entry(city).or_default().add(measurement);
    }

    write!(std::io::stdout(), "{{").unwrap();
    cities.into_iter().for_each(|(city, s)| {
        let (min, avg, max) = s.complete();
        write!(
            std::io::stdout(),
            "\"{city}\": {{\"min\": {min:.1}, \"avg\": {avg:.1}, \"max\": {max:.1}}},",
        )
        .unwrap();
    });
    write!(std::io::stdout(), "}}").unwrap();
    std::io::stdout().flush().unwrap();
}
