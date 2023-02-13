use rand::prelude::*;

/// Returns a sequence of [A-Za-z]
pub fn ran_letters(len: usize) -> String {
    let mut string = String::with_capacity(len);
    let mut rng = thread_rng();
    for _ in 0..len {
        string.push({
            let byte = rng.gen_range(0..58) + 65_u8;
            (if 90 < byte && byte < 97 {
                byte + 7 // Some brackets and other squigles are between the upper and lower case letters
            } else {
                byte
            }) as char
        });
    }
    string
}
