use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{self, BufWriter, Write},
};

use rand::RngExt;
use rand_distr::{Distribution, Normal};

// Java's Math.Round, which rounds towards positive infinity.
fn round(v: f64) -> f64 {
    (v + 0.5).floor()
}

fn main() -> io::Result<()> {
    let rows = env::args()
        .nth(1)
        .map(|v| v.parse().unwrap())
        .unwrap_or(1_000_000_000u64);
    let stations: Vec<(String, Normal<f64>)> = fs::read_to_string("data/weather_stations.csv")?
        .lines()
        .skip(3)
        .filter(|s| !s.is_empty())
        .map(|l| {
            let (station, temperature) = l.split_once(";").unwrap();
            let mean: f64 = temperature.parse().unwrap();
            let dist = Normal::new(mean, 10.0).unwrap();
            (station.to_string(), dist)
        })
        .collect();

    let mut rng = rand::rng();
    let mut out = BufWriter::with_capacity(1 << 16, File::create("measurements.txt")?);
    let mut expected: HashMap<&str, (f64, f64, f64, u64)> = HashMap::with_capacity(stations.len());
    for _ in 0..rows {
        let (station, dist) = &stations[rng.random_range(0..stations.len())];
        let sample = {
            let sample = dist.sample(&mut rng).clamp(-99.9, 99.9);
            (sample * 10.0).round() / 10.0
        };
        let (min, max, sum, cnt) = expected
            .entry(station)
            .or_insert((f64::MAX, f64::MIN, 0.0, 0));
        *min = min.min(sample);
        *max = max.max(sample);
        *sum += sample;
        *cnt += 1;
        writeln!(out, "{station};{sample:.1}")?;
    }
    out.flush()?;

    let mut out = File::create("expected")?;
    write!(out, "{{")?;
    for (i, (city, (min, max, sum, cnt))) in expected.into_iter().enumerate() {
        // https://github.com/gunnarmorling/1brc/blob/main/src/main/java/dev/morling/onebrc/CalculateAverage_baseline.java
        let mean = (round(sum * 10.0) / 10.0) / cnt as f64;
        let (min, mean, max) = (
            round(min * 10.0) / 10.0,
            round(mean * 10.0) / 10.0,
            round(max * 10.0) / 10.0,
        );
        if i == 0 {
            write!(out, "{city}={min:.1}/{mean:.1}/{max:.1}",)?;
        } else {
            write!(out, ", {city}={min:.1}/{mean:.1}/{max:.1}")?;
        }
    }
    write!(out, "}}")?;
    Ok(())
}
