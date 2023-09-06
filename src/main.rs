mod profiler;
use memmap2::{Mmap, MmapOptions};
fn main() {
    //profiler::pagefinder::some_stuff(2);
    let size = profiler::utils::get_phys_memory_size();

    let mut mmap = MmapOptions::new()
        .len(16638189568)
        .map_anon()
        .expect("error");
    println!("Map: {:#?}", mmap);

    let mut ptr = mmap.as_mut_ptr();
    println!("Ptr before: {:#?}", ptr);

    unsafe {
        println!("Before change: {}", *ptr);
        *ptr = 1;
        println!("After change: {}", *ptr);
        ptr = ptr.offset(1);
    }

    println!("Ptr after: {:#?}", ptr);
    println!("SIZE: {}", size);
}
