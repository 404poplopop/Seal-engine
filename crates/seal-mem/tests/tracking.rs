//! Runs with the tracking allocator installed as this test binary's global allocator.
use seal_mem::{snapshot, tagged, MemTag, TrackingAllocator};

#[global_allocator]
static ALLOC: TrackingAllocator<std::alloc::System> = TrackingAllocator::new(std::alloc::System);

fn live(tag: MemTag) -> u64 {
    snapshot().iter().find(|u| u.tag == tag).unwrap().live_bytes
}

#[test]
fn allocations_are_charged_to_their_tag_and_credited_back_across_threads() {
    let before = live(MemTag::Physics);
    let block = tagged(MemTag::Physics, || vec![0u8; 1 << 20]);
    assert!(live(MemTag::Physics) >= before + (1 << 20));
    // Freed on another thread under no tag: still credited to Physics.
    std::thread::spawn(move || drop(block)).join().unwrap();
    assert!(live(MemTag::Physics) < before + (1 << 20));
    let peak = snapshot().iter().find(|u| u.tag == MemTag::Physics).unwrap().peak_bytes;
    assert!(peak >= 1 << 20);
}

#[test]
fn over_aligned_allocations_stay_aligned() {
    #[repr(align(64))]
    struct Wide([u8; 64]);
    let items: Vec<Box<Wide>> = (0..32).map(|_| Box::new(Wide([1; 64]))).collect();
    assert!(items.iter().all(|b| (&**b as *const Wide as usize) % 64 == 0 && b.0[63] == 1));
    let mut grow = tagged(MemTag::Assets, Vec::<u64>::new);
    for i in 0..10_000 {
        grow.push(i);
    }
    assert_eq!(grow.iter().sum::<u64>(), 49_995_000);
}
