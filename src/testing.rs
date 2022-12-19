use std::path::PathBuf;

use rand::{thread_rng, prelude::*};

fn main() {
    // println!("{:?}", latest("./"));
    println!("{}", ran_letters(32));
}

fn latest(dir: &str) -> Option<PathBuf> {
    let files = match std::fs::read_dir(dir) {
        Ok(files) => files,
        Err(_) => return None,
    }
    .into_iter();

    match files
        .flatten()
        .map(|f| f.path())
        .map(|p| {
            let time = match std::fs::metadata(&p) {
                Ok(metadata) => match metadata.modified() {
                    Ok(time) => time,
                    Err(_) => return None,
                },
                Err(_) => return None,
            };
            Some((p, time))
        })
        .flatten()
        .max_by_key(|t| t.1)
    {
        Some(tuple) => Some(tuple.0),
        None => None,
    }
}

fn ran_letters(len: usize) -> String{
    let mut string = String::with_capacity(len) ;
    let mut rng = thread_rng();
    for _ in 0..len {
        string.push(
            match rng.gen_range(0..26) {
                0 => 'a',
                1 => 'b',
                2 => 'c',
                3 => 'd',
                4 => 'e',
                5 => 'f',
                6 => 'g',
                7 => 'h',
                8 => 'i',
                9 => 'j',
                10 => 'k', 
                11 => 'l',
                12 => 'm',
                13 => 'n',
                14 => 'o',
                15 => 'p',
                16 => 'q',
                17 => 'r',
                18 => 's',
                19 => 't',
                20 => 'u',
                21 => 'v',
                22 => 'w',
                23 => 'x',
                24 => 'y',
                _ => 'z',
            }
        );
    }

    string
}
