use std::{collections::HashMap, fs};

use serde::Deserialize;

#[derive(Deserialize, PartialEq, Debug)]
struct CityResult {
    min: f64,
    avg: f64,
    max: f64,
}

fn main() {
    let mine = {
        let mine = fs::read_to_string("out.json").unwrap();
        let results: HashMap<String, CityResult> = serde_json::from_str(&mine).unwrap();
        results
    };
    let expected = {
        let mine = fs::read_to_string("expected.json").unwrap();
        let results: HashMap<String, CityResult> = serde_json::from_str(&mine).unwrap();
        results
    };

    assert_eq!(mine.len(), expected.len());
    for (city, result) in mine {
        assert_eq!(Some(&result), expected.get(&city), "{city}");
    }
}
