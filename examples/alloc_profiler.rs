use {
    std::time::Duration,
    wc::alloc::{
        profiler::{self, JemallocMultiBinFilter},
        Jemalloc,
    },
};

/// Configure profiler allocator to track specific allocation bins (4096 and
/// 8192 bytes).
#[global_allocator]
static ALLOCATOR: profiler::Alloc<Jemalloc, JemallocMultiBinFilter<2>> =
    profiler::Alloc::new(Jemalloc, JemallocMultiBinFilter::new([4096, 8192]));

fn allocate(capacity: usize) -> Vec<u8> {
    Vec::<u8>::with_capacity(capacity)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let handle = tokio::spawn(profiler::record(Duration::from_millis(500)));

    // Give some time for tokio to actually execute the profiler future.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Allocate memory.
    let mut _buffer = allocate(256); // This one should not be recorded.
    let mut _buffer = allocate(4096);
    let mut _buffer = allocate(8192);

    // Obtain JSON-serialized DHAT profile.
    let profile = handle.await.unwrap().unwrap();

    eprintln!("{profile}");

    Ok(())
}
