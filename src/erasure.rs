// use std::{
//     fs::{self, File, OpenOptions},
//     io::{self, BufReader, BufWriter, Read, Seek, SeekFrom},
//     path::PathBuf,
//     str::FromStr,
// };

// use anyhow::anyhow;
// use glob::glob;
// use ndarray::prelude::*;
// use num_bigint::{BigInt, Sign};
// use once_cell::sync::Lazy;
// use rayon::prelude::*;
// use regex::Regex;
// use serde::{Deserialize, Serialize};
// use tracing::{debug, info};

// use crate::my_br::MyBigRational;

// static PRECISE_CTRL1: usize = 65537;

// #[derive(Clone, Serialize, Deserialize)]
// pub struct MetaData {
//     pub data_parts: usize,
//     pub erasure_parts: usize,
// }

// static REGEX_FOR_METADATA: Lazy<Regex> =
//     Lazy::new(|| Regex::new(r"^(\d+)\+(\d+)$").expect("regex should be compiled"));

// impl FromStr for MetaData {
//     type Err = anyhow::Error;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         let caps = REGEX_FOR_METADATA
//             .captures(s)
//             .ok_or(anyhow!("mailformed pattern: {s}"))?;

//         let data_parts: usize = caps
//             .get(1)
//             .ok_or(anyhow!("pattern missing data_parts"))?
//             .as_str()
//             .parse()?;
//         let erasure_parts: usize = caps
//             .get(2)
//             .ok_or(anyhow!("pattern missing erasure_parts"))?
//             .as_str()
//             .parse()?;

//         Ok(MetaData {
//             data_parts,
//             erasure_parts,
//         })
//     }
// }

// #[derive(Debug)]
// pub struct DataBlock(usize, MyBigRational);

// impl DataBlock {
//     pub fn load_from_file(file_name: &PathBuf) -> anyhow::Result<Self> {
//         let order = extract_order_from_file_path(&file_name)?;
//         let buf = fs::read(file_name)?;
//         let buf = MyBigRational::from_bytes(&buf);
//         Ok(DataBlock(order, buf))
//     }

//     /// save_to_file will save the data to disk
//     /// the reason of comsume self is to drop the memory
//     pub fn save_to_file(self, dir_name: &PathBuf, meta_ref: &MetaData) -> anyhow::Result<()> {
//         let order = self.0;
//         let file_path = Self::get_file_path(order, dir_name, meta_ref);
//         let bytes = self.1.to_bytes();
//         fs::write(file_path, bytes)?;

//         Ok(())
//     }

//     pub fn get_file_path(order: usize, dir_name: &PathBuf, meta_ref: &MetaData) -> PathBuf {
//         let file_name = if order < meta_ref.data_parts {
//             format!("{order}.d.block")
//         } else {
//             format!("{order}.e.block")
//         };
//         dir_name.join(file_name)
//     }
// }

// fn extract_order_from_file_path(file_path: &PathBuf) -> anyhow::Result<usize> {
//     let file_name = file_path
//         .file_name()
//         .ok_or(anyhow!("missing file name"))?
//         .to_string_lossy();
//     let prefix = &file_name[..file_name.len() - ".x.block".len()];
//     Ok(prefix.parse()?)
// }

// #[derive(Debug)]
// pub struct ErasureEntity {
//     matrix: Array2<BigInt>,
// }

// impl ErasureEntity {
//     pub fn load_from_blocks(mut blocks: Vec<DataBlock>) -> anyhow::Result<Self> {
//         let block_size = blocks.len();
//         let max_order = block_size - 1;
//         let shape = (block_size, block_size + 1);

//         // sort by order
//         blocks.sort_by_key(|block| block.0);
//         let mut matrix_raw_vec: Vec<BigInt> = Vec::with_capacity(shape.0 * shape.1);
//         debug!("loading data blocks");
//         for block in blocks {
//             // first push data, then ascend order
//             let curr_order = block.0;
//             let row_vector = build_vector_with_data(curr_order, max_order, block.1);
//             matrix_raw_vec.extend(row_vector);
//         }

//         let matrix = Array2::from_shape_vec(shape, matrix_raw_vec)?;
//         debug!("solving equations");
//         let matrix = solve_equation(matrix);
//         Ok(Self { matrix })
//     }

//     pub fn calc_data(&self, order: usize) -> DataBlock {
//         let (r, _) = self.matrix.dim();
//         let vector = build_vector(order, r);
//         let x_vector = Array1::from_vec(vector);
//         let co_column = self.matrix.column(0);
//         let data: BigInt = co_column
//             .iter()
//             .zip(x_vector.iter())
//             .map(|(a, b)| a * b)
//             .sum();
//         DataBlock(order, data >> PRECISE_CTRL1)
//     }
// }

// fn build_vector(curr_order: usize, max_order: usize) -> Vec<BigInt> {
//     let mut ret = Vec::with_capacity(max_order);
//     for o in 0..=max_order {
//         ret.push(curr_order.pow(o.try_into().unwrap()).into());
//     }
//     ret
// }

// fn build_vector_with_data(curr_order: usize, max_order: usize, data: BigInt) -> Vec<BigInt> {
//     let mut ret = Vec::with_capacity(max_order + 1);
//     ret.push(data << PRECISE_CTRL1);
//     let vector = build_vector(curr_order, max_order);
//     ret.extend(vector);
//     ret
// }

// fn solve_equation(mut eq_mat: Array2<BigInt>) -> Array2<BigInt> {
//     let (r, _) = eq_mat.dim();
//     make_zero(&mut eq_mat, 1);
//     make_identity(&mut eq_mat, r - 2);
//     eq_mat
// }

// fn make_zero(eq_mat: &mut Array2<BigInt>, start: usize) {
//     let (r, _) = eq_mat.dim();
//     if start == r {
//         return;
//     }
//     for r_idx in (start..r).rev() {
//         let prev_row = eq_mat.row(r_idx - 1).to_owned();
//         let mut this_row = eq_mat.row_mut(r_idx);
//         this_row -= &prev_row;
//         let scale = this_row[start + 1].clone();
//         this_row.map_inplace(|x| *x /= &scale);
//     }
//     make_zero(eq_mat, start + 1)
// }

// fn make_identity(eq_mat: &mut Array2<BigInt>, start: usize) {
//     let (r, _) = eq_mat.dim();
//     for minus_on in (start + 1..r).rev() {
//         let this_row = eq_mat.row(start);
//         let scale = (&this_row[minus_on + 1]).to_owned();
//         if scale == 0.into() {
//             continue;
//         }
//         let minus_row = eq_mat.row(minus_on).map(|v| v * &scale);
//         let mut this_row = eq_mat.row_mut(start);
//         this_row -= &minus_row;
//     }

//     if start != 0 {
//         make_identity(eq_mat, start - 1);
//     }
// }

// /// FileSplitter split the file into parts
// pub struct FileHandler {
//     metadata: MetaData,
//     file_path: PathBuf,
//     dir_path: PathBuf,
// }

// impl FileHandler {
//     pub fn new(metadata: MetaData, file_path: PathBuf, dir_path: PathBuf) -> Self {
//         Self {
//             metadata,
//             file_path,
//             dir_path,
//         }
//     }

//     fn split_and_write(&self) -> anyhow::Result<()> {
//         // calculating chunks and chunk size;
//         let src_file = File::open(&self.file_path)?;
//         let src_file_size = src_file.metadata()?.len();
//         let MetaData { data_parts, .. } = self.metadata;
//         let chunk_size = src_file_size as usize / data_parts;

//         let paths: Vec<_> = (0..data_parts)
//             .map(|order| DataBlock::get_file_path(order, &self.dir_path, &self.metadata))
//             .collect();
//         let result: anyhow::Result<Vec<_>> = paths
//             .par_iter()
//             .enumerate()
//             .map(|(chunk_idx, dest_path)| -> anyhow::Result<()> {
//                 let dest_file = OpenOptions::new()
//                     .write(true)
//                     .create(true)
//                     .open(dest_path)?;

//                 let mut dest_writer = BufWriter::new(dest_file);
//                 let start_pos = chunk_idx * chunk_size;
//                 let bytes_to_copy = if chunk_idx == data_parts - 1 {
//                     src_file_size - start_pos as u64
//                 } else {
//                     chunk_size as u64
//                 };

//                 let mut src_file = src_file.try_clone()?;
//                 src_file.seek(SeekFrom::Start(start_pos as u64))?;
//                 let mut src_reader = BufReader::new(src_file).take(bytes_to_copy);

//                 io::copy(&mut src_reader, &mut dest_writer)?;

//                 Ok(())
//             })
//             .collect();
//         let _ = result?;
//         Ok(())
//     }

//     /// reconstruct will split the file into parts and add erasure parts as configured in metadata
//     pub fn reconstruct(&self) -> anyhow::Result<()> {
//         debug!("start loading file");
//         let MetaData {
//             data_parts,
//             erasure_parts,
//         } = self.metadata;

//         debug!(data_parts, erasure_parts, "start splitting files");
//         self.split_and_write()?;

//         debug!("start loading blocks");
//         let paths = self.get_block_paths()?;
//         let data_blocks = paths
//             .into_par_iter()
//             .map(|file_path| {
//                 let block_buf = fs::read(&file_path).expect("should read file");
//                 let buf = BigInt::from_bytes_le(Sign::Plus, &block_buf);
//                 let order =
//                     extract_order_from_file_path(&file_path).expect("order should be extracted");
//                 DataBlock(order, buf)
//             })
//             .collect();

//         // save erasure parts to disk
//         debug!("loading erasure entity");
//         let ee = ErasureEntity::load_from_blocks(data_blocks)?;
//         self.gen_and_save_blocks(&ee)?;

//         // save metadata to disk
//         let md_path = self.dir_path.join("metadata.json");
//         let buf = serde_json::to_string(&self.metadata)?;
//         fs::write(md_path, buf)?;

//         Ok(())
//     }

//     fn gen_and_save_blocks(&self, ee: &ErasureEntity) -> anyhow::Result<()> {
//         let MetaData {
//             data_parts,
//             erasure_parts,
//         } = self.metadata;
//         (0..data_parts + erasure_parts)
//             .into_par_iter()
//             .for_each(|order| {
//                 // let file_path = DataBlock::get_file_path(order, &self.dir_path, &self.metadata);
//                 // if file_path.exists() {
//                 //     let file_path = file_path.to_string_lossy().into_owned();
//                 //     warn!(file_path, "file exists, no need to calc and rebuild again");
//                 //     return;
//                 // }

//                 info!(order, "start calculating");
//                 let calced_block = ee.calc_data(order);
//                 info!(order, "calc finished, start saving");
//                 calced_block
//                     .save_to_file(&self.dir_path, &self.metadata)
//                     .expect("failed to save file");
//             });

//         Ok(())
//     }

//     fn get_block_paths(&self) -> anyhow::Result<Vec<PathBuf>> {
//         let p = self.dir_path.join("*.block").to_string_lossy().into_owned();
//         let may_matched = glob(&p)?;
//         let paths: Vec<_> = may_matched.into_iter().filter_map(|m| m.ok()).collect();
//         Ok(paths)
//     }

//     /// rebuild will search for the metadata.json and load the blocks
//     /// if least number of blocks is not satisfied, a `not enough blocks` error will be returned
//     pub fn rebuild(&self, force: bool) -> anyhow::Result<()> {
//         let paths = self.get_block_paths()?;
//         let MetaData { data_parts, .. } = self.metadata;
//         let block_count = paths.len();
//         if block_count < data_parts {
//             return Err(anyhow!(
//                 "not enough blocks, got {block_count}, required at least {data_parts}"
//             ));
//         }

//         debug!("loading blocks");
//         let blocks: Vec<_> = paths
//             .into_par_iter()
//             .map(|ref path| {
//                 DataBlock::load_from_file(path).expect(&format!("should load file {path:?}"))
//             })
//             .take(data_parts)
//             .collect();

//         debug!("setup equation matrix");
//         let ee = ErasureEntity::load_from_blocks(blocks)?;
//         self.gen_and_save_blocks(&ee)?;

//         let file_dest_exists = self.file_path.exists();
//         if file_dest_exists && !force {
//             return Err(anyhow!("dest file exists, please rerun with `--force` to overwrite the dest file"))
//         }

//         info!("start assambling file from blocks");
//         let mut file_dest = OpenOptions::new().append(true).to_owned();
//         if !file_dest_exists {
//             file_dest.create(true);
//         }
//         let file_dest = file_dest.open(&self.file_path)?;
//         let mut file_dest = BufWriter::new(file_dest);

//         let p = self
//             .dir_path
//             .join("*.d.block")
//             .to_string_lossy()
//             .into_owned();
//         let may_matched = glob(&p)?;
//         let mut paths: Vec<_> = may_matched.into_iter().filter_map(|m| m.ok()).collect();
//         paths.sort_by_key(|p| extract_order_from_file_path(&p).expect("order should exists"));
//         for p in paths {
//             let file_parts = File::open(p)?;
//             let mut file_parts = BufReader::new(file_parts);
//             io::copy(&mut file_parts, &mut file_dest)?;
//         }

//         info!("all done!");

//         Ok(())
//     }
// }
