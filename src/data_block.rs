use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    ops::{MulAssign, Neg},
    path::PathBuf,
    thread::{self, JoinHandle},
};

use num_bigint::{BigInt, BigUint, Sign};
use num_rational::Ratio;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

mod sign_serde {
    use num_bigint::Sign;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(data: &Sign, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Custom serialization logic here
        // For example, you can use serializer.serialize_str(...) or any other serialization method
        // serializer.serialize_bytes(data)
        let data_str = match data {
            Sign::Minus => "minus",
            Sign::NoSign => "no_sign",
            Sign::Plus => "plus",
        };
        serializer.serialize_str(data_str)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Sign, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Custom deserialization logic here
        // For example, you can use deserializer.deserialize_str(...) or any other deserialization method
        // deserializer.deserialize_bytes()
        let s = String::deserialize(deserializer)?;
        let ret = match &*s {
            "minus" => Sign::Minus,
            "no_sign" => Sign::NoSign,
            "plus" => Sign::Plus,
            _ => {
                return Err(serde::de::Error::unknown_variant(
                    &s,
                    &["minus", "no_sign", "plus"],
                ))
            }
        };
        Ok(ret)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBlockMeta {
    // data_parts is total data_parts count
    data_parts: u64,
    // curr_part is the current part number
    curr_part: u64,
    // erasure_parts is the erasure total parts number
    erasure_parts: u64,

    #[serde(with = "sign_serde")]
    sign: Sign,
    padding: usize,

    // work_dir
    work_dir: PathBuf,
}

static PART_EXTRACTOR: Lazy<Regex> = Lazy::new(|| {
    let re = r"^(\d+)\.[de]\.block";
    Regex::new(re).expect("it should be compiled")
});

impl DataBlockMeta {
    // create的时候根据参数加载
    pub fn load_from_params(
        work_dir: PathBuf,
        data_parts: u64,
        curr_part: u64,
        erasure_parts: u64,
    ) -> Self {
        Self {
            data_parts,
            curr_part,
            erasure_parts,

            sign: Sign::Plus,
            padding: 0, // 稍后读取文件的时候会覆写

            work_dir,
        }
    }

    // rebuild的时候，从meta.json加载
    pub fn load_from_file(work_dir: &PathBuf) -> anyhow::Result<Vec<Self>> {
        let meta_file_path = work_dir.join("meta.json");
        let meta_bytes = fs::read(meta_file_path)?;
        let ret: Vec<Self> = serde_json::from_slice(&meta_bytes)?;
        Ok(ret)
    }

    fn is_erasure_type(&self) -> bool {
        self.curr_part >= self.data_parts
    }

    fn get_file_path(&self) -> PathBuf {
        let ext = if self.is_erasure_type() {
            "e.block"
        } else {
            "d.block"
        };
        let curr_part = self.curr_part;
        let file_name = format!("{curr_part}.{ext}");
        self.work_dir.join(file_name)
    }

    pub fn calc_L_item_k_on_x(&self, xs: &[u64], x: u64) -> (u64, Ratio<i64>) {
        let j = self.curr_part;
        assert!(
            xs.contains(&j),
            "j should present in xs while calc L item, j: {j}, xs: {xs:?}"
        );

        let r = calc_lagrange_item_at_x(xs, j, x);
        (j, r)
    }
}

fn interpolate_one(sorted_blocks: &[DataBlock], xs: &[u64], part_num: u64) -> BigInt {
    assert_ne!(sorted_blocks.len(), 0, "blocks shouldn't be empty");

    let b0_meta = &sorted_blocks[0].meta;
    let data_parts = b0_meta.data_parts;

    let ratio_list = sorted_blocks
        .iter()
        .map(|block| block.meta.calc_L_item_k_on_x(xs, part_num));

    sorted_blocks
        .iter()
        .map(|b| (b.meta.curr_part, &b.data))
        .zip(ratio_list)
        .fold(
            BigInt::from(0),
            |acc, ((curr_part, big_data), (part_num, ratio))| {
                assert_eq!(curr_part, part_num);
                acc + big_data * ratio.numer() / ratio.denom()
            },
        )
}

pub fn interpolate_all_and_dump(blocks: &mut Vec<DataBlock>, xs: &[u64]) -> anyhow::Result<Vec<DataBlock>> {
    assert_ne!(blocks.len(), 0, "blocks shouldn't be empty");
    blocks.sort_by_key(|b| b.meta.curr_part);
    let b0_meta = &blocks[0].meta.clone();
    let data_parts = b0_meta.data_parts;
    let erasure_parts = b0_meta.erasure_parts;

    let mut rebuilt_blocks = vec![];
    assert!(xs.len() as u64 >= data_parts, "not enough blocks");

    for part_num in 0..data_parts + erasure_parts {
        if xs.contains(&part_num) {
            info!(part_num, "exists, no need to rebuild");
            continue;
        }
        let rebuild_data = interpolate_one(&blocks, &xs, part_num);
        let mut rebuild_meta = b0_meta.clone();
        rebuild_meta.curr_part = part_num;
        rebuild_meta.sign = rebuild_data.sign();

        if rebuild_meta.is_erasure_type() {
            // no need to padding for erasure type
            rebuild_meta.padding = 0;
        }

        let rebuild_block = DataBlock {
            data: rebuild_data,
            meta: rebuild_meta,
        };
        if !xs.contains(&part_num) {
            rebuilt_blocks.push(rebuild_block);
        }
    }

    Ok(rebuilt_blocks)
}

fn get_part_number_from_file_name(p: &PathBuf) -> usize {
    let file_name = p
        .file_name()
        .expect("file name should exists")
        .to_string_lossy();
    if let Some(captures) = PART_EXTRACTOR.captures(&file_name) {
        if let Some(number_str) = captures.get(1) {
            if let Ok(number) = number_str.as_str().parse::<usize>() {
                return number;
            }
        }
    }
    panic!("it should be parsed, got file name: {file_name}");
}

fn calc_lagrange_item_at_x(xs: &[u64], j: u64, x: u64) -> Ratio<i64> {
    let j = j as i64;
    let x = x as i64;
    xs.iter()
        .map(|xi| *xi as i64)
        .filter(|&xi| xi != j)
        .map(|xi| Ratio::new(x - xi, j - xi))
        .fold(Ratio::from(1), |acc, c| acc * c)
}

pub struct DataBlock {
    data: BigInt,
    meta: DataBlockMeta,
}

impl DataBlock {
    pub fn calc_lagrange_interpolation(&self, xs: &[u64]) -> BigInt {
        let x = self.meta.curr_part;
        assert!(
            !xs.contains(&x),
            "x should not present in xs while calc L interpolation, x: {x}, xs: {xs:?}"
        );

        todo!()
    }

    pub fn calc_lagrange_item(&self, xs: &[u64], x: u64) -> BigInt {
        let j = self.meta.curr_part;
        assert!(
            xs.contains(&j),
            "j should present in xs while calc L item, j: {j}, xs: {xs:?}"
        );

        let k = calc_lagrange_item_at_x(xs, j, x);
        let numer = k.numer();
        let denom = k.denom();
        &self.data * numer / denom
    }

    // load_data will loads the data from file.
    // filename indicated by self
    // step: Vec::with_capacity
    // step: load numer and denom
    // step: transmute
    pub fn load_data(mut meta: DataBlockMeta) -> anyhow::Result<Self> {
        let file_path = &meta.get_file_path();

        // step: Vec::with_capacity
        let raw_data_len = fs::metadata(file_path)?.len() as usize;
        // last one is tail padding (which to prevent starting zeros of a numer)
        let tail_padded_len = if meta.is_erasure_type() {
            // 如果是计算出的纠删码，那没必要tail padding，因为计算出的肯定是leading_zero去掉了（小端存储，末尾去0）
            raw_data_len
        } else {
            raw_data_len + 1
        };
        let head_padding_size = if meta.is_erasure_type() {
            0
        } else {
            calc_padding_size(tail_padded_len as usize)
        };
        meta.padding = head_padding_size;
        let vec_cap = head_padding_size + tail_padded_len;
        let mut data = vec![0xffu8; vec_cap];

        // step: load biguint
        let mut file = File::open(file_path)?;
        let raw_section = &mut data[head_padding_size..raw_data_len + head_padding_size];
        file.read_exact(raw_section)?;

        info!("vec -> bu, part: {}", meta.curr_part);
        let bu = BigUintFitter::from_vec(data);
        // let bu = BigUint::from_bytes_le(&data);
        let data = BigInt::from_biguint(meta.sign, bu);

        Ok(Self { data, meta })
    }

    // dump_data will store the data to disk
    // filename indicated by self
    // transmute to vec
    pub fn dump_data(self) -> anyhow::Result<()> {
        let Self { mut data, meta } = self;

        let file_path = meta.get_file_path();
        // delete this file first
        if let Err(err) = fs::remove_file(&file_path) {
            info!(err = err.to_string(), "suppres the deletion error");
        }
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&file_path)?;

        // exam if last head_padded is not 0xff. if not, data should minus 1
        if !meta.is_erasure_type() {
            let rebuilt_padded_value: Vec<_> =
                data.iter_u64_digits().take(meta.padding / 8).collect();
            let &last_one = rebuilt_padded_value
                .last()
                .expect("it should not be empty vec");
            if last_one != !0u64 {
                data -= BigInt::from(1);
            }
        }

        // transmute data to vec
        let bu = match data.to_biguint() {
            Some(bu) => bu,
            None => data.neg().to_biguint().expect("it shouldn't be None"),
        };

        info!("bu -> vec, part: {}", meta.curr_part);
        let mut data = BigUintFitter::to_vec(bu);
        // let mut data = bu.to_bytes_le();
        let data_len = data.len();

        // unpadding
        // meta.padding is head_padding
        // last one is tail padding (which to prevent starting zeros of a numer)
        // 如果是计算出的纠删码，那没必要tail padding，因为计算出的肯定是leading_zero去掉了（小端存储，末尾去0）
        let end = if meta.is_erasure_type() {
            data_len
        } else {
            data_len - 1
        };
        let write_section = &mut data[meta.padding..end];
        file.write_all(write_section)?;

        Ok(())
    }
}

pub fn save_meta(blocks: &[DataBlock]) -> anyhow::Result<()> {
    assert!(blocks.len() != 0);
    let meta_list: Vec<_> = blocks.iter().map(|b| &b.meta).collect();
    let m0 = meta_list[0];
    let work_dir = &m0.work_dir;
    let file_path = work_dir.join("meta.json");

    let json_str = serde_json::to_string_pretty(&meta_list)?;
    fs::write(file_path, json_str)?;
    Ok(())
}

// BigUintFitter is used to load data directly from the mem.
// It's using unsafe code to avoid mem copy.
struct BigUintFitter {
    data: Vec<u64>,
}

impl BigUintFitter {
    // from_vec is used to build BigInt from file
    fn from_vec(mut bytes: Vec<u8>) -> BigUint {
        info!("from_vec before convert: {bytes:?}");
        // u64_len is 8
        let u64_len = std::mem::size_of::<u64>();
        assert_eq!(bytes.len() % u64_len, 0);
        // transmute Vec<u8> to Vec<u64>
        let u64_len = bytes.len() / u64_len;
        let u64_vec: Vec<u64>;
        unsafe {
            let raw_ptr: *const u8 = bytes.as_ptr();
            let u64_ptr: *const u64 = raw_ptr as *const u64;
            // may be a little bit mem leekage
            u64_vec = Vec::from_raw_parts(u64_ptr as *mut u64, u64_len, u64_len);
            std::mem::forget(bytes);

            info!("from_vec after convert: {u64_vec:?}");

            let fitter = Self { data: u64_vec };
            let ret = std::mem::transmute(fitter);
            info!("from_vec after, bu: {ret}");
            ret
        }
    }

    // to_vec is used to store BigInt to file
    fn to_vec(bu: BigUint) -> Vec<u8> {
        unsafe {
            info!("to_vec before, bu: {bu}");
            // step 1. transmute bu to fitter
            let fitter: Self = std::mem::transmute(bu);
            let Self { data } = fitter;

            info!("to_vec before convert: {data:?}");
            // step 2. transmute Vec<u64> to Vec<u8>
            let u8_len = data.len() * std::mem::size_of::<u64>();
            let raw_u64: *const u64 = data.as_ptr();
            let raw_u8: *const u8 = raw_u64 as *const u8;
            std::mem::forget(data);
            let u8_vec = Vec::from_raw_parts(raw_u8 as *mut u8, u8_len, u8_len);

            info!("to_vec after convert: {u8_vec:?}");

            u8_vec
        }
    }

    #[allow(unused)]
    fn normalize(&mut self) {
        if let Some(&0) = self.data.last() {
            let len = self.data.iter().rposition(|&d| d != 0).map_or(0, |i| i + 1);
            self.data.truncate(len);
        }
        if self.data.len() < self.data.capacity() / 4 {
            self.data.shrink_to_fit();
        }
    }
}

// 后方无脑+1， 前方看情况，所以此处计算得到的是前方padding的0xff数量，且至少是8 * 2个，
// 这样可以覆盖掉u64余数，如果后面补的那个经过计算后不是0xff，那么说明计算后的数反而大了，
// 要减去1
fn calc_padding_size(tail_padded_len: usize) -> usize {
    8 - tail_padded_len % 8 + 8 * 2
}

#[cfg(test)]
mod tests {
    use std::{fs::{File, self}, io::Read};
    use num_bigint::Sign;
    use num_rational::Ratio;
    use tracing::info;
    use super::{
        calc_lagrange_item_at_x, interpolate_all_and_dump, save_meta, BigUintFitter, DataBlock,
        DataBlockMeta,
    };

    #[test]
    fn test_integration1() {
        tracing_subscriber::fmt::init();

        let work_dir = "/tmp/erasure_test";
        let data_parts = 2;
        let erasure_parts = 1;

        // let xs = [1u64, 3, 4];
        let xs = [0u64, 1];
        // let xs = [2u64, 1];
        let meta_list = xs
            .into_iter()
            .map(|idx| DataBlockMeta {
                data_parts,
                erasure_parts,
                curr_part: idx,
                padding: 0,
                sign: Sign::Plus,
                work_dir: work_dir.into(),
            })
            .collect::<Vec<_>>();

        let mut blocks = meta_list
            .into_iter()
            .map(|meta| DataBlock::load_data(meta).unwrap())
            .collect::<Vec<_>>();

        let rebuilt_blocks = interpolate_all_and_dump(&mut blocks, &xs).unwrap();
        blocks.extend(rebuilt_blocks);
        blocks.sort_by_key(|b| b.meta.curr_part);
        save_meta(&blocks).unwrap();
        for b in blocks {
            if !xs.contains(&b.meta.curr_part) {
                b.dump_data().unwrap();
            }
        }
    }

    #[test]
    fn test_integration2() {
        tracing_subscriber::fmt::init();

        let xs = [2u64, 0];

        let meta_path = "/tmp/erasure_test/meta.json";
        let meta_data = fs::read(meta_path).unwrap();
        let meta_list: Vec<DataBlockMeta> = serde_json::from_slice(&meta_data).unwrap();
        let mut blocks = meta_list
            .into_iter()
            .filter(|meta| xs.contains(&meta.curr_part))
            .map(|meta| DataBlock::load_data(meta).unwrap())
            .collect::<Vec<_>>();

        let rebuilt_blocks = interpolate_all_and_dump(&mut blocks, &xs).unwrap();
        blocks.extend(rebuilt_blocks);
        blocks.sort_by_key(|b| b.meta.curr_part);
        save_meta(&blocks).unwrap();
        for b in blocks {
            if !xs.contains(&b.meta.curr_part) {
                b.dump_data().unwrap();
            }
        }
    }

    #[test]
    fn test_lagrange3() {
        let xs = vec![0, 1, 2, 3];
        let ys = vec![b'g' as u64, b'x' as u64, b'd' as u64, b'n' as u64];
        let mut sum = Ratio::from_integer(0);
        for (&j, y) in xs.iter().zip(ys) {
            let k = calc_lagrange_item_at_x(&xs, j, 2);
            sum += k * y as i64;
        }
        println!("{sum}");

        let mut sum = Ratio::from_integer(0);
        let xs = vec![1, 3, 4, 5];
        let ys = vec![b'x' as u64, b'n' as u64, 217, 488];
        for (&j, y) in xs.iter().zip(ys) {
            let k = calc_lagrange_item_at_x(&xs, j, 2);
            sum += k * y as i64;
        }
        println!("{sum}");
    }

    #[test]
    fn test_lagrange4() {
        let xs = (0..=20u64).collect::<Vec<_>>();
        for j in 0..24 {
            for x in 0..24 {
                let k = calc_lagrange_item_at_x(&xs, j, x);
                println!("j = {j}, x = {x} => k = {k}");
            }
            println!("===============\n")
        }
    }

    #[test]
    fn test_transmute() {
        let meta = DataBlockMeta {
            data_parts: 1,
            erasure_parts: 1,
            curr_part: 0,
            sign: Sign::Plus,
            padding: 0,
            work_dir: "/tmp/erasure_test".into(),
        };

        let mut d = DataBlock::load_data(meta).unwrap();
        d.meta.curr_part = 100;
        d.dump_data().unwrap();
    }
}
