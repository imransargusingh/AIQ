// Spike S2: keyring 4.x (v1 API) behaviour on Linux without a Secret Service.
use keyring::Entry;
use std::time::Instant;
fn main() {
    let t = Instant::now();
    println!("store_status: {:?}", Entry::store_status());
    match Entry::new("aiq-spike", "store-key") {
        Err(e) => println!("new: Err({e:?})"),
        Ok(en) => {
            println!("set_secret: {:?}", en.set_secret(&[7u8; 32]));
            println!("get_secret: {:?}", en.get_secret().map(|v| v.len()));
            println!("delete: {:?}", en.delete_credential());
        }
    }
    println!("elapsed_ms: {}", t.elapsed().as_millis());
}
