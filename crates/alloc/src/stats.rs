use {
    serde::Deserialize,
    tikv_jemalloc_ctl::{epoch, stats_print},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Jemalloc error: {0}")]
    Jemalloc(#[from] tikv_jemalloc_ctl::Error),

    #[error("Failed to write stats: {0}")]
    Stats(std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
pub struct TotalStats {
    pub allocated: u64,
    pub active: u64,
    pub metadata: u64,
    pub resident: u64,
    pub mapped: u64,
    pub retained: u64,
}

#[derive(Debug, Deserialize)]
pub struct BinStats {
    pub nmalloc: u64,
    pub ndalloc: u64,
    pub nrequests: u64,
}

#[derive(Debug, Deserialize)]
pub struct MergedArenaStats {
    pub bins: Vec<BinStats>,
}

#[derive(Debug, Deserialize)]
pub struct ArenaStats {
    pub merged: MergedArenaStats,
}

#[derive(Debug, Deserialize)]
pub struct BinConstants {
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct ArenaConstants {
    pub bin: Vec<BinConstants>,
}

#[derive(Debug, Deserialize)]
pub struct JemallocStats {
    #[serde(rename = "stats")]
    pub total: TotalStats,

    #[serde(rename = "arenas")]
    pub arena_constants: ArenaConstants,

    #[serde(rename = "stats.arenas")]
    pub arena_stats: ArenaStats,
}

#[derive(Debug, Deserialize)]
struct GlobalStats {
    jemalloc: JemallocStats,
}

pub fn collect_jemalloc_stats() -> Result<JemallocStats, Error> {
    epoch::advance()?;

    let mut opts = tikv_jemalloc_ctl::stats_print::Options::default();
    opts.json_format = true;
    opts.skip_per_arena = true;
    opts.skip_mutex_statistics = true;

    let mut buf = vec![];
    stats_print::stats_print(&mut buf, opts).map_err(Error::Stats)?;

    let global: GlobalStats = serde_json::from_slice(&buf[..])?;

    Ok(global.jemalloc)
}

#[cfg(feature = "metrics")]
pub fn update_jemalloc_metrics() -> Result<(), Error> {
    use metrics::backend::gauge;

    let stats = collect_jemalloc_stats()?;
    let total = &stats.total;

    // Total number of bytes allocated by the application. This corresponds to
    // `stats.allocated` in jemalloc's API.
    gauge!("jemalloc_memory_allocated").set(total.allocated as f64);

    // Total number of bytes in active pages allocated by the application. This
    // corresponds to `stats.active` in jemalloc's API.
    gauge!("jemalloc_memory_active").set(total.active as f64);

    // Total number of bytes dedicated to `jemalloc` metadata. This corresponds to
    // `stats.metadata` in jemalloc's API.
    gauge!("jemalloc_memory_metadata").set(total.metadata as f64);

    // Total number of bytes in physically resident data pages mapped by the
    // allocator. This corresponds to `stats.resident` in jemalloc's API.
    gauge!("jemalloc_memory_resident").set(total.resident as f64);

    // Total number of bytes in active extents mapped by the allocator. This
    // corresponds to `stats.mapped` in jemalloc's API.
    gauge!("jemalloc_memory_mapped").set(total.mapped as f64);

    // Total number of bytes in virtual memory mappings that were retained rather
    // than being returned to the operating system via e.g. `munmap(2)`. This
    // corresponds to `stats.retained` in jemalloc's API.
    gauge!("jemalloc_memory_retained").set(total.retained as f64);

    let bin_const = stats.arena_constants.bin.iter();
    let bin_stats = stats.arena_stats.merged.bins.iter();

    for (bin_const, bin_stats) in bin_const.zip(bin_stats) {
        let gauge =
            |name, value| gauge!(name, "bin_size" => bin_const.size.to_string()).set(value as f64);

        // Cumulative number of times a bin region of the corresponding size class was
        // allocated from the arena, whether to fill the relevant tcache if opt.tcache
        // is  enabled, or to directly satisfy an allocation request otherwise.
        gauge("jemalloc_memory_bin_nmalloc", bin_stats.nmalloc);

        // Cumulative number of times a bin region of the corresponding size class was
        // returned to the arena, whether to flush the relevant tcache if opt.tcache is
        // enabled, or to directly deallocate an allocation otherwise.
        gauge("jemalloc_memory_bin_ndalloc", bin_stats.ndalloc);

        // Cumulative number of allocation requests satisfied by bin regions of the
        // corresponding size class.
        gauge("jemalloc_memory_bin_nrequests", bin_stats.nrequests);

        gauge(
            "jemalloc_memory_bin_nactive",
            bin_stats.nmalloc - bin_stats.ndalloc,
        );
    }

    Ok(())
}
