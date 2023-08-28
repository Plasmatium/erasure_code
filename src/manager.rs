use crate::data_block::{DataBlock, DataBlockMeta};
use anyhow::anyhow;
use num_bigint::Sign;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{fs, path::PathBuf, str::FromStr};

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

        let blocks = meta_list
            .into_iter()
            .filter_map(|meta| DataBlock::load_data(meta).ok())
            .collect::<Vec<_>>();

        Ok(Self { blocks })
    }

    pub fn build_erasure_file(&self) -> anyhow::Result<()> {
        todo!()
    }

    pub fn reconstruct(&self) -> anyhow::Result<()> {
        todo!()
    }
}

static REGEX_FOR_METADATA: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\d+)\+(\d+)$").expect("regex should be compiled"));

pub struct PartsParam(u64, u64);

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
