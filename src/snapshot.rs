use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use memmapix::MmapMut;
use std::collections::VecDeque;

use std::fs::OpenOptions;

use std::fs::File;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Default)]
enum ChunkState {
    #[default]
    Todo,
    InProgress,
    Failed,
    Invalid,
    Done,
}

impl From<u8> for ChunkState {
    fn from(u: u8) -> Self {
        match u {
            0b0000_0000 => ChunkState::Todo,
            0b0100_0000 => ChunkState::InProgress,
            0b1000_0000 => ChunkState::Failed,
            0b1111_1111 => ChunkState::Done,
            //            0b100 => ChunkState::Done,     // :HACK:
            0b100 => ChunkState::Todo, // :HACK:
            //            0b1 => ChunkState::InProgress, // :HACK:
            _ => ChunkState::Invalid,
        }
    }
}

impl From<ChunkState> for u8 {
    fn from(cs: ChunkState) -> Self {
        match cs {
            ChunkState::Todo => 0b0000_0000,
            ChunkState::InProgress => 0b0100_0000,
            ChunkState::Failed => 0b1000_0000,
            ChunkState::Invalid => 0b1111_1111,
            ChunkState::Done => 0b1111_1111,
        }
    }
}

#[derive(Debug)]
struct ChunkMap {
    number_of_chunks: usize,
    mmap: MmapMut,
}

impl ChunkMap {
    pub fn open(name: &str, number_of_chunks: usize) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&name)?;

        file.set_len(number_of_chunks as u64)?;

        let mmap = unsafe { MmapMut::map_mut(&file)? };
        let s = Self {
            number_of_chunks,
            mmap,
        };

        Ok(s)
    }

    pub fn for_each_todo<F>(&self, mut f: F) -> anyhow::Result<()>
    where
        F: FnMut(usize, ChunkState) -> anyhow::Result<()>,
    {
        for (i, c) in self.mmap.iter().enumerate() {
            let cs = ChunkState::from(*c);
            match cs {
                ChunkState::Todo => f(i, cs)?,
                _ => {}
            }
        }
        Ok(())
    }

    pub fn for_each_inprogress<F>(&self, mut f: F) -> anyhow::Result<()>
    where
        F: FnMut(usize, ChunkState) -> anyhow::Result<()>,
    {
        for (i, c) in self.mmap.iter().enumerate() {
            let cs = ChunkState::from(*c);
            // eprintln!("{} -> {:?} ({:#b})", i, cs, c);
            match cs {
                ChunkState::InProgress => f(i, cs)?,
                _ => {}
            }
        }
        Ok(())
    }

    pub fn set_chunk_state(&mut self, i: usize, s: ChunkState) -> anyhow::Result<()> {
        if i >= self.number_of_chunks {
            anyhow::bail!("Out of bounds")
        }

        self.mmap[i] = u8::from(s); // as u8;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Snapshot {
    id: String,
    progress: Option<MultiProgress>,
    r#continue: bool,
}

const BLOCKS_PER_CHUNK: usize = 100; // >=100 as per AWS API
const BLOCK_SIZE: usize = 524288; // 512KiB
const CHUNK_SIZE: usize = BLOCK_SIZE * BLOCKS_PER_CHUNK;

impl Snapshot {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            progress: None,
            r#continue: false,
        }
    }

    pub fn enable_continue(&mut self) {
        self.r#continue = true;
    }

    pub fn use_progress(&mut self, m: MultiProgress) {
        self.progress = Some(m);
    }

    pub async fn download(&mut self) -> anyhow::Result<()> {
        let config = aws_config::load_from_env().await;
        let ec2_client = aws_sdk_ec2::Client::new(&config);

        let snapshots = ec2_client.describe_snapshots().snapshot_ids(&self.id);

        let snapshots = snapshots.send().await?;

        let size_in_bytes; // = 0;
        if let Some(snapshots) = snapshots.snapshots {
            if let Some((_description, _state, size)) = snapshots.iter().find_map(|s| {
                // this is a bit silly since we should expect exactly one result
                if s.snapshot_id != Some(self.id.clone()) {
                    None
                } else {
                    //dbg!(s);
                    Some((s.description.clone(), s.state.clone(), s.volume_size))
                }
            }) {
                //dbg!(description);
                //dbg!(state);
                //dbg!(size);
                //let size = None::<()>;
                size.expect("Volume size is needed");

                let size = size.unwrap() as usize;
                size_in_bytes = size * 1_073_741_824;

                tracing::info!("Downloading {}GiB", size)
            } else {
                anyhow::bail!("Snapshot {} not found", &self.id);
            }
        } else {
            //tracing::warn!("Snapshot {} not found", &self.id );
            anyhow::bail!("Snapshot {} not found", &self.id);
        }

        let filename = format!("./{}.img", &self.id);
        let path = Path::new(&filename);
        let mut f = match path.try_exists() {
            Ok(true) => {
                // check continue
                if !self.r#continue {
                    anyhow::bail!("{filename} exists, but 'continue' was not requested");
                }
                OpenOptions::new().write(true).open(&path)?
            }
            Ok(false) => {
                // create
                File::create(&path)?
            }
            Err(o) => {
                tracing::error!("Failed verifying if {filename} exists -> {o}");
                anyhow::bail!("Failed verifying if {filename} exists -> {o}")
            }
        };

        //let mut f = File::create(&path)?;
        //let mut f = OpenOptions::new().write(true).open(&path)?;
        // preallocate the file on disk
        f.set_len(size_in_bytes as u64)?;

        /*
        // for 0 byte at end?
        f.seek(SeekFrom::Start(size_in_bytes as u64 -1))?;
        f.write(&[0])?;
        */

        let chunks = size_in_bytes / CHUNK_SIZE;

        let map_name = format!("./{}.omsmap", &self.id);

        let mut chunk_map = ChunkMap::open(&map_name, chunks)?;

        tracing::info!("Queing {} chunks", chunks);

        let mut chunk_queue = VecDeque::new();

        //        chunk_queue.push_back(129);

        chunk_map.for_each_inprogress(|i, _s| {
            dbg!(i);
            chunk_queue.push_back(i);
            Ok(())
        })?;

        chunk_map.for_each_todo(|i, _s| {
            chunk_queue.push_back(i);
            Ok(())
        })?;

        let chunk_progress = if let Some(mp) = &self.progress {
            let cl = chunk_queue.len();

            let sty = ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} [{eta_precise}] {msg}",
            )
            .unwrap()
            .progress_chars("##-");

            let progress = mp.add(ProgressBar::new(cl as u64));
            progress.set_style(sty.clone());

            Some(progress)
        } else {
            None
        };

        while let Some(c) = chunk_queue.pop_front() {
            //tracing::info!("Downloading chunk {} / {}", c, chunks);
            if let Some(pb) = &chunk_progress {
                pb.set_message(format!("Downloading chunk {} / {}", c, chunks));
            }

            {
                // :TODO: extract
                chunk_map.set_chunk_state(c, ChunkState::InProgress)?;

                let block_progress = if let Some(mp) = &self.progress {
                    let sty = ProgressStyle::with_template(
                        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} [{eta_precise}] {msg}",
                    )
                    .unwrap()
                    .progress_chars("##-");

                    let progress = mp.add(ProgressBar::new(BLOCKS_PER_CHUNK as u64));
                    progress.set_style(sty.clone());

                    Some(progress)
                } else {
                    None
                };

                let client = aws_sdk_ebs::Client::new(&config);

                let first_block_in_chunk = (c * BLOCKS_PER_CHUNK) as i32;
                let last_block_in_chunk =
                    (first_block_in_chunk + BLOCKS_PER_CHUNK as i32 - 1) as i32;

                let list = client
                    .list_snapshot_blocks()
                    .snapshot_id(&self.id)
                    .starting_block_index(first_block_in_chunk)
                    .max_results(BLOCKS_PER_CHUNK as i32);
                let list = list.send().await?;

                // :TODO: verify block size
                //dbg!(&list);
                //dbg!(&list.blocks);
                for block in &list.blocks.unwrap() {
                    match (block.block_index, &block.block_token) {
                        (Some(i), Some(t)) => {
                            // Note: snapshots are sparse, so empty blocks will be skipped
                            // resulting in bleeding into the next chunk here
                            // Plan: A different approach on slicing/chunking this might be better

                            if i >= first_block_in_chunk && i <= last_block_in_chunk {
                                // tracing::info!("Downloading block {}", i);
                                if let Some(pb) = &block_progress {
                                    pb.set_message(format!(
                                        "Downloading block {} [{}-{}]",
                                        i, first_block_in_chunk, last_block_in_chunk
                                    ));
                                }

                                let block = client
                                    .get_snapshot_block()
                                    .snapshot_id(&self.id)
                                    .block_index(i)
                                    .block_token(t);

                                let block = block.send().await?;

                                //dbg!(block);
                                let p = i as u64 * BLOCK_SIZE as u64;

                                f.seek(SeekFrom::Start(p as u64))?;
                                //        let r = u8::read_from(block.block_data)?;
                                let data = block.block_data.collect().await?;
                                //io::copy(&mut data, &mut f)?;
                                f.write(&data.into_bytes())?;

                                if let Some(block_progress) = &block_progress {
                                    block_progress.inc(1);
                                }

                                // for now just stop after one block
                                //todo!("die");
                            }
                        }
                        _ => {
                            // :TODO:
                        }
                    }
                }
                chunk_map.set_chunk_state(c, ChunkState::Done)?;
                if let Some(chunk_progress) = &chunk_progress {
                    chunk_progress.inc(1);
                }
            }
            // for now just stop after one chunk
            //            todo!("die");
        }

        /*
                let block = client.get_snapshot_block().snapshot_id( &self.id ).block_index(0).block_token("TODO");

                let block = block.send().await?;

                dbg!(block);
        */
        Ok(())
    }
}
