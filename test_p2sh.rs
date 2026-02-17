fn main() {
    let addr = "3J98t1WpEZ73CNmYviecrnyiWrnqRhWNLy";
    println!("Address: {}", addr);
    println!("Length: {}", addr.len());
    println!("Starts with 3: {}", addr.starts_with('3'));
    
    // Try to decode
    let decoded = bs58::decode(addr).into_vec();
    match decoded {
        Ok(v) => {
            println!("Decoded length: {}", v.len());
            println!("Version byte: {:#x}", v[0]);
            if v.len() >= 25 {
                println!("Checksum in data: {:?}", &v[21..25]);
            }
        }
        Err(e) => println!("Decode error: {}", e),
    }
}
