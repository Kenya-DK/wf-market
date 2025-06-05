use rand::Rng;
use std::format;

/**
Generate a random Device ID to identify your device in the backend

# Notes
Result of this should be stored and reused upon usage, this will uniquely identify
every device used on the account

# Returns
A random ID of format `d-${16 random characters}`
*/
pub fn generate_device_id() -> String {
    let mut rng = rand::rng();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
    let random_string: String = (0..16)
        .map(|_| chars[rng.gen_range(0..chars.len())])
        .collect();
    
    format!("d-{}", random_string)
}