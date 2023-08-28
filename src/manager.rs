use crate::data_block::{interpolate_all_and_dump, save_meta, DataBlock, DataBlockMeta};
use anyhow::anyhow;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom},
    path::PathBuf,
    str::FromStr,
};
use tracing::error;

pub struct Manager {
    blocks: Vec<DataBlock>,
}

impl Manager {
    pub fn new(parts_params: PartsParam, work_dir: &PathBuf) -> anyhow::Result<Self> {
        let PartsParam(data_parts, erasure_parts) = parts_params;
        let blocks = (0..data_parts)
            .map(|curr_part| {
                DataBlockMeta::load_from_params(
                    work_dir.clone(),
                    data_parts,
                    curr_part,
                    erasure_parts,
                )
            })
            .map(|meta| DataBlock::load_data(meta))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self { blocks })
    }

    pub fn load_from_meta(meta_file_path: &PathBuf) -> anyhow::Result<Self> {
        let meta_data = fs::read(meta_file_path)?;
        let meta_list: Vec<DataBlockMeta> = serde_json::from_slice(&meta_data)?;

        let mut blocks = meta_list
            .into_iter()
            .filter_map(|meta| DataBlock::load_data(meta).ok())
            .collect::<Vec<_>>();
        blocks.sort_by_key(|b| b.get_curr_part());

        Ok(Self { blocks })
    }

    pub fn reconstruct_parts(mut self) -> anyhow::Result<()> {
        let mut xs = self
            .blocks
            .iter()
            .map(|b| b.get_curr_part())
            .collect::<Vec<_>>();
        xs.sort();
        let rebuilt_blocks = interpolate_all_and_dump(&mut self.blocks, &xs)?;
        self.blocks.extend(rebuilt_blocks);
        self.blocks.sort_by_key(|b| b.get_curr_part());
        save_meta(&self.blocks)?;
        for b in self.blocks {
            if !xs.contains(&b.get_curr_part()) {
                b.dump_data()?;
            }
        }
        Ok(())
    }

    pub fn split_file_to_parts(
        input_file_name: &PathBuf,
        data_parts: u64,
        work_dir: &PathBuf,
    ) -> anyhow::Result<()> {
        // calculating chunks and chunk size;
        let src_file = File::open(input_file_name)?;
        let src_file_size = src_file.metadata()?.len();
        let chunk_size = src_file_size / data_parts;

        let paths = (0..data_parts)
            .into_iter()
            .map(|part_num| {
                let file_name = format!("{part_num}.d.block");
                work_dir.join(file_name)
            })
            .collect::<Vec<_>>();

        let result: anyhow::Result<Vec<_>> = paths
            .par_iter()
            .enumerate()
            .map(|(chunk_idx, dest_path)| -> anyhow::Result<()> {
                let chunk_idx = chunk_idx as u64;
                let dest_file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .open(dest_path)?;

                let mut dest_writer = BufWriter::new(dest_file);
                let start_pos = chunk_idx * chunk_size;
                let bytes_to_copy = if chunk_idx == data_parts - 1 {
                    src_file_size - start_pos as u64
                } else {
                    chunk_size as u64
                };

                let mut src_file = src_file.try_clone()?;
                src_file.seek(SeekFrom::Start(start_pos as u64))?;
                let mut src_reader = BufReader::new(src_file).take(bytes_to_copy);

                io::copy(&mut src_reader, &mut dest_writer)?;

                Ok(())
            })
            .collect();
        let _ = result?;
        Ok(())
    }

    pub fn merge_parts_to_file(
        output_file_name: &PathBuf,
        data_dir: &PathBuf,
        data_parts: u64,
    ) -> anyhow::Result<()> {
        let _ = fs::remove_file(output_file_name);

        let paths = (0..data_parts)
            .map(|part| {
                let file_name = format!("{part}.d.block");
                data_dir.join(file_name)
            })
            .collect::<Vec<_>>();

        let mut dest_file = OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(output_file_name)?;

        for path in paths {
            let mut src_file = File::open(path)?;
            io::copy(&mut src_file, &mut dest_file)?;
        }

        Ok(())
    }

    // return layout: (data_parts: u64, erasure_parts: u64)
    pub fn get_parts_params(&self) -> (u64, u64) {
        assert!(self.blocks.len() != 0);
        let (data_parts, _, erasure_parts) = self.blocks[0].get_parts_params();
        (data_parts, erasure_parts)
    }
}

static REGEX_FOR_METADATA: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\d+)\+(\d+)$").expect("regex should be compiled"));

// PartsParams layout: (data_parts: u64, erasure_parts: u64)
#[derive(Clone, Copy)]
pub struct PartsParam(pub u64, pub u64);

impl FromStr for PartsParam {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let caps = REGEX_FOR_METADATA
            .captures(s)
            .ok_or(anyhow!("mailformed pattern: {s}"))?;

        let data_parts: u64 = caps
            .get(1)
            .ok_or(anyhow!("pattern missing data_parts"))?
            .as_str()
            .parse()?;
        let erasure_parts: u64 = caps
            .get(2)
            .ok_or(anyhow!("pattern missing erasure_parts"))?
            .as_str()
            .parse()?;

        Ok(Self(data_parts, erasure_parts))
    }
}
