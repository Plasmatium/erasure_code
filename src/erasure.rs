use std::{fs, path::PathBuf, str::FromStr};

use anyhow::anyhow;
use ndarray::prelude::*;
use num_bigint::{BigInt, Sign};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{info, debug, warn};

static PRECISE_CTRL: usize = 65536;

#[derive(Clone, Serialize, Deserialize)]
pub struct MetaData {
    pub data_parts: usize,
    pub erasure_parts: usize,
}

static REGEX_FOR_METADATA: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\d+)\+(\d+)$").expect("regex should be compiled"));

impl FromStr for MetaData {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let caps = REGEX_FOR_METADATA
            .captures(s)
            .ok_or(anyhow!("mailformed pattern: {s}"))?;

        let data_parts: usize = caps
            .get(1)
            .ok_or(anyhow!("pattern missing data_parts"))?
            .as_str()
            .parse()?;
        let erasure_parts: usize = caps
            .get(2)
            .ok_or(anyhow!("pattern missing erasure_parts"))?
            .as_str()
            .parse()?;

        Ok(MetaData {
            data_parts,
            erasure_parts,
        })
    }
    // type Error = anyhow::Error;

    // fn try_from(value: &str) -> Result<Self, Self::Error> {
    //     let caps = REGEX_FOR_METADATA
    //         .captures(value)
    //         .ok_or(anyhow!("mailformed pattern: {value}"))?;

    //     let data_parts: usize = caps
    //         .get(1)
    //         .ok_or(anyhow!("pattern missing data_parts"))?
    //         .as_str()
    //         .parse()?;
    //     let erasure_parts: usize = caps
    //         .get(2)
    //         .ok_or(anyhow!("pattern missing erasure_parts"))?
    //         .as_str()
    //         .parse()?;

    //     Ok(MetaData {
    //         data_parts,
    //         erasure_parts,
    //     })
    // }
}

#[derive(Debug)]
pub struct DataBlock(pub usize, pub BigInt);

impl DataBlock {
    pub fn load_from_file(file_name: PathBuf) -> anyhow::Result<Self> {
        let order = extract_order_from_file_name(&file_name)?;
        let buf = fs::read(file_name)?;
        let buf = BigInt::from_bytes_le(Sign::Plus, &buf);
        Ok(DataBlock(order, buf))
    }

    /// save_to_file will save the data to disk
    /// the reason of comsume self is to drop the memory
    pub fn save_to_file(self, dir_name: &PathBuf, meta_ref: &MetaData) -> anyhow::Result<()> {
        let order = self.0;
        let file_path = Self::get_file_path(order, dir_name, meta_ref);
        let (_, bytes) = self.1.to_bytes_le();
        fs::write(file_path, bytes)?;

        Ok(())
    }

    pub fn get_file_path(order: usize, dir_name: &PathBuf, meta_ref: &MetaData) -> PathBuf {
        let file_name = if order < meta_ref.data_parts {
            format!("{order}.d")
        } else {
            format!("{order}.e")
        };
        dir_name.join(file_name)
    }
}

fn extract_order_from_file_name(file_name: &PathBuf) -> anyhow::Result<usize> {
    let stem = file_name
        .file_stem()
        .ok_or("malformed file name")
        .map_err(|e| anyhow!(e))?;
    Ok(stem.to_string_lossy().parse()?)
}

#[derive(Debug)]
pub struct ErasureEntity {
    matrix: Array2<BigInt>,
}

impl ErasureEntity {
    pub fn load_from_blocks(mut blocks: Vec<DataBlock>) -> anyhow::Result<Self> {
        let block_size = blocks.len();
        let max_order = block_size - 1;
        let shape = (block_size, block_size + 1);

        // sort by order
        blocks.sort_by_key(|block| block.0);
        let mut matrix_raw_vec: Vec<BigInt> = Vec::with_capacity(shape.0 * shape.1);
        debug!("loading data blocks");
        for block in blocks {
            // first push data, then ascend order
            let curr_order = block.0;
            let row_vector = build_vector_with_data(curr_order, max_order, block.1);
            matrix_raw_vec.extend(row_vector);
        }

        let matrix = Array2::from_shape_vec(shape, matrix_raw_vec)?;
        debug!("solving equations");
        let matrix = solve_equation(matrix);
        Ok(Self { matrix })
    }

    pub fn calc_data(&self, order: usize) -> DataBlock {
        let (r, _) = self.matrix.dim();
        let vector = build_vector(order, r);
        let x_vector = Array1::from_vec(vector);
        let co_column = self.matrix.column(0);
        let data: BigInt = co_column
            .iter()
            .zip(x_vector.iter())
            .map(|(a, b)| a * b)
            .sum();
        DataBlock(order, data >> PRECISE_CTRL)
    }

    pub fn gen_erasure_file(&self, count: usize) -> Vec<DataBlock> {
        let mut ret = Vec::with_capacity(count);
        let (r, _) = self.matrix.dim();

        for o in r..r + count {
            let data_block = self.calc_data(o);
            ret.push(data_block);
        }
        ret
    }
}

fn build_vector(curr_order: usize, max_order: usize) -> Vec<BigInt> {
    let mut ret = Vec::with_capacity(max_order);
    for o in 0..=max_order {
        ret.push(curr_order.pow(o.try_into().unwrap()).into());
    }
    ret
}

fn build_vector_with_data(curr_order: usize, max_order: usize, data: BigInt) -> Vec<BigInt> {
    let mut ret = Vec::with_capacity(max_order + 1);
    ret.push(data << PRECISE_CTRL);
    let vector = build_vector(curr_order, max_order);
    ret.extend(vector);
    ret
}

fn solve_equation(mut eq_mat: Array2<BigInt>) -> Array2<BigInt> {
    let (r, _) = eq_mat.dim();
    make_zero(&mut eq_mat, 1);
    make_identity(&mut eq_mat, r - 2);
    eq_mat
}

fn make_zero(eq_mat: &mut Array2<BigInt>, start: usize) {
    let (r, _) = eq_mat.dim();
    if start == r {
        return;
    }
    for r_idx in (start..r).rev() {
        let prev_row = eq_mat.row(r_idx - 1).to_owned();
        let mut this_row = eq_mat.row_mut(r_idx);
        this_row -= &prev_row;
        let scale = this_row[start + 1].clone();
        this_row.map_inplace(|x| *x /= &scale);
    }
    make_zero(eq_mat, start + 1)
}

fn make_identity(eq_mat: &mut Array2<BigInt>, start: usize) {
    let (r, _) = eq_mat.dim();
    for minus_on in (start + 1..r).rev() {
        let this_row = eq_mat.row(start);
        let scale = (&this_row[minus_on + 1]).to_owned();
        if scale == 0.into() {
            continue;
        }
        let minus_row = eq_mat.row(minus_on).map(|v| v * &scale);
        let mut this_row = eq_mat.row_mut(start);
        this_row -= &minus_row;
    }

    if start != 0 {
        make_identity(eq_mat, start - 1);
    }
}

/// FileSplitter split the file into parts
pub struct FileHandler {
    metadata: MetaData,
    file_path: PathBuf,
    dir_path: PathBuf,
}

impl FileHandler {
    pub fn new(metadata: MetaData, file_path: PathBuf, dir_path: PathBuf) -> Self {
        Self {
            metadata,
            file_path,
            dir_path,
        }
    }

    /// split will split the file into parts and add erasure parts as configured in metadata
    pub fn split(&self) -> anyhow::Result<()> {
        debug!("start loading file");
        let file_buf = fs::read(&self.file_path)?;
        let MetaData {
            data_parts,
            erasure_parts,
        } = self.metadata;
        let file_size = file_buf.len();
        let block_size = file_size / data_parts;

        debug!(data_parts, erasure_parts, "start splitting file");
        let data_blocks = (0..data_parts)
            .into_par_iter()
            .map(|part_idx| {
                let start = part_idx * block_size;
                let end = if part_idx == data_parts - 1 {
                    // last parts, catch all of remain bytes
                    file_size
                } else {
                    start + block_size
                };
                (part_idx, &file_buf[start..end])
            })
            .map(|(order, block_buf)| {
                let buf = BigInt::from_bytes_le(Sign::Plus, block_buf);
                DataBlock(order, buf)
            })
            .collect();

        // save erasure parts to disk
        debug!("loading erasure entity");
        let ee = ErasureEntity::load_from_blocks(data_blocks)?;
        (0..data_parts + erasure_parts)
            .into_par_iter()
            .for_each(|order| {
                let file_path = DataBlock::get_file_path(order, &self.dir_path, &self.metadata);
                if file_path.exists() {
                    let file_path = file_path.to_string_lossy().into_owned();
                    warn!(file_path, "file exists, no need to calc and rebuild again");
                    return
                }

                info!(order, "start calculating");
                let calced_block = ee.calc_data(order);
                info!(order, "calc finished, start saving");
                calced_block
                    .save_to_file(&self.dir_path, &self.metadata)
                    .expect("failed to save file");
            });

        // save metadata to disk
        let md_path = self.dir_path.join("metadata.json");
        let buf = serde_json::to_string(&self.metadata)?;
        fs::write(md_path, buf)?;

        Ok(())
    }

    pub fn rebuild(&self) -> anyhow::Result<()> {
        todo!()
    }
}
