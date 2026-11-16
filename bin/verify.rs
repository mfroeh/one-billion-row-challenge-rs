use std::{collections::HashMap, fs};

#[derive(PartialEq, Debug)]
struct CityResult {
    min: f64,
    mean: f64,
    max: f64,
}

impl CityResult {
    fn parse(from: &str) -> (String, CityResult) {
        let (city, res) = from.split_once('=').unwrap();
        let res: Vec<f64> = res.split('/').map(|s| s.parse().unwrap()).collect();
        (
            city.trim().to_string(),
            CityResult {
                min: res[0],
                mean: res[1],
                max: res[2],
            },
        )
    }
}

fn parse_results(string: String) -> HashMap<String, CityResult> {
    let out = discard_surrounding(string);
    out.split(',').map(CityResult::parse).collect()
}

fn discard_surrounding(string: String) -> String {
    let mut chars = string.chars();
    chars.next();
    chars.next_back();
    chars.collect()
}

fn main() {
    let out = fs::read_to_string("out").unwrap();
    let out = parse_results(out);

    let expected = fs::read_to_string("expected").unwrap();
    let expected = parse_results(expected);

    assert_eq!(out.len(), expected.len());
    for (city, want) in expected {
        assert_eq!(out.get(&city), Some(&want), "{city}");
    }
}
