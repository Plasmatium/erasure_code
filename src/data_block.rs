use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    ops::{MulAssign, Neg},
    path::PathBuf,
};

use num_bigint::{BigInt, BigUint, Sign, ToBigUint};
use num_rational::{BigRational, Ratio};
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::info;

// data layout
// meta: [numer_size][numer_padding_num][denorm_padding_num]
// main: [numer][numer_padding][numer_sign][denom][denorm_padding][denom_sign]
// all: [meta][main]
// numer_padding_num is count by u64, which means numer_padding_num * 8 in bytes
// denom is calculated in the same way.

#[derive(Debug, Clone)]
pub struct DataBlockMeta {
    // data_parts is total data_parts count
    data_parts: u64,
    // curr_part is the current part number
    curr_part: u64,
    // erasure_parts is the erasure total parts number
    erasure_parts: u64,

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
    pub fn new(work_dir: &PathBuf, k_parts: usize, erasure_parts: usize) -> anyhow::Result<Self> {
        todo!()
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

fn calc_lagrange_list(xs: &[i64], j: i64, x: i64) -> Ratio<i64> {
    xs.iter()
        .filter(|&&xi| xi != j)
        .map(|xi| Ratio::new(x - xi, j - xi))
        .fold(Ratio::from(1), |acc, c| acc * c)
}

pub struct DataBlock {
    data: BigInt,
    meta: DataBlockMeta,
}

impl DataBlock {
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
        let tail_padded_len = raw_data_len + 1;
        let head_padding_size = calc_padding_size(tail_padded_len as usize);
        meta.padding = head_padding_size;
        let vec_cap = head_padding_size + tail_padded_len;
        let mut data = vec![0xffu8; vec_cap];

        // step: load biguint
        let mut file = File::open(file_path)?;
        let raw_section = &mut data[head_padding_size..raw_data_len + head_padding_size];
        file.read_exact(raw_section)?;

        let bu = BigUintFitter::from_vec(data);
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
        let rebuilt_padded_value: Vec<_> = data.iter_u64_digits().take(meta.padding / 8).collect();
        let &last_one = rebuilt_padded_value
            .last()
            .expect("it should not be empty vec");
        if last_one != !0u64 {
            data -= BigInt::from(1);
        }

        // transmute data to vec
        let bu = match data.to_biguint() {
            Some(bu) => bu,
            None => data.neg().to_biguint().expect("it shouldn't be None"),
        };
        let mut data = BigUintFitter::to_vec(bu);
        let data_len = data.len();

        // unpadding
        // meta.padding is head_padding
        // last one is tail padding (which to prevent starting zeros of a numer)
        let write_section = &mut data[meta.padding..data_len - 1];
        file.write_all(write_section)?;

        Ok(())
    }
}

// BigUintFitter is used to load data directly from the mem.
// It's using unsafe code to avoid mem copy.
struct BigUintFitter {
    data: Vec<u64>,
}

impl BigUintFitter {
    // from_vec is used to build BigInt from file
    fn from_vec(mut bytes: Vec<u8>) -> BigUint {
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

            let mut fitter = Self { data: u64_vec };
            std::mem::transmute(fitter)
        }
    }

    // to_vec is used to store BigInt to file
    fn to_vec(bu: BigUint) -> Vec<u8> {
        unsafe {
            // step 1. transmute bu to fitter
            let fitter: Self = std::mem::transmute(bu);
            let Self { data } = fitter;

            // step 2. transmute Vec<u64> to Vec<u8>
            let u8_len = data.len() * std::mem::size_of::<u64>();
            let raw_u64: *const u64 = data.as_ptr();
            let raw_u8: *const u8 = raw_u64 as *const u8;
            std::mem::forget(data);
            let mut u8_vec = Vec::from_raw_parts(raw_u8 as *mut u8, u8_len, u8_len);

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
    use std::{fs::File, io::Read};

    use dashu::{
        base::BitTest,
        integer::{IBig, UBig},
        rational::RBig,
    };
    use num_bigint::{BigInt, BigUint, Sign};
    use num_rational::{BigRational, Ratio};
    use tracing::info;

    use super::{calc_lagrange_list, BigUintFitter, DataBlock, DataBlockMeta};

    fn make_meta() -> DataBlockMeta {
        DataBlockMeta {
            data_parts: 20,
            curr_part: 1,
            erasure_parts: 4,
            sign: Sign::Plus,
            padding: 0,
            work_dir: "/tmp/erasure_test".into(),
        }
    }

    #[test]
    fn test1() {
        let meta = make_meta();
        let mut block = DataBlock::load_data(meta).unwrap();
        block.meta.curr_part = 3;
        block.dump_data().unwrap();
    }

    #[test]
    fn test2() {
        let a = 0u64;
        println!("{:b}", !a);
    }

    #[test]
    fn test_dashu() {
        use dashu::integer::Sign;
        let mut file = File::open("/tmp/erasure_test/1.d.block").unwrap();
        let mut data = vec![];
        file.read_to_end(&mut data).unwrap();
        println!("{} size read", data.len());
        let a = UBig::from_le_bytes(&data);
        let numer = IBig::from_parts(Sign::Positive, a);
        let denom = UBig::from_le_bytes(&data[(data.len() / 10)..(data.len() / 3 * 2)]);
        let xx = numer / denom;
        println!("{}", xx.bit_len());
        // let rational = RBig::from_parts(numer, denom);
        // println!("{:?}", rational.sign())
    }

    #[test]
    fn test_lagrange() {
        let xs: Vec<i64> = (0..=20).into_iter().collect();
        for j in 0..24 {
            for x in 0..24 {
                let l = calc_lagrange_list(&xs, j, x);
                println!("{x}, {j} => {l}");
            }
            println!("=================================\n")
        }
        // let x = 6;
        // let j = 3;
        // let xx = calc_lagrange_list(&xs, j, x);
        // println!("{xx}");
    }
}
