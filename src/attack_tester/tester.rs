extern "C" {
    fn crypto_kem_keypair(pk: *mut u8, sk: *mut u8) -> i32;
    fn crypto_kem_enc(ct: *mut u8, ss: *mut u8, pk: *const u8) -> i32;
    fn crypto_kem_dec(ss: *mut u8, ct: *const u8, sk: *const u8) -> i32;
}

const CRYPTO_PUBLICKEYBYTES: usize = 9616;
const CRYPTO_SECRETKEYBYTES: usize = 19888;
const CRYPTO_BYTES: usize = 16;
const CRYPTO_CIPHERTEXTBYTES: usize = 9720;

pub(crate) fn main() {
    let mut pk = [0u8; CRYPTO_PUBLICKEYBYTES];
    let mut sk = [0u8; CRYPTO_SECRETKEYBYTES];
    let mut ss_encap = [0u8; CRYPTO_BYTES];
    let mut ss_decap = [0u8; CRYPTO_BYTES];
    let mut ct = [0u8; CRYPTO_CIPHERTEXTBYTES];

    //println!("pk before: {:?}", pk);
    //println!("sk before: {:?}", sk);
    unsafe {
        crypto_kem_keypair(pk.as_mut_ptr(), sk.as_mut_ptr());
        crypto_kem_enc(ct.as_mut_ptr(), ss_encap.as_mut_ptr(), pk.as_ptr());
        crypto_kem_dec(ss_decap.as_mut_ptr(), ct.as_ptr(), sk.as_ptr());

        if ss_encap == ss_decap {
            println!("Success!");
        } else {
            println!("Failure!");
        }
    }
}
