mod erasure;

use std::fs;

use num_bigint::{BigInt, Sign};
use crate::erasure::{DataBlock, ErasureEntity};


fn main() {
    let mut data_blocks = vec![];
    for i in 0..5 {
        let file_name = format!("src/test_pics/origin/{i}.png");
        let data = fs::read(file_name).unwrap();
        let block = BigInt::from_bytes_be(Sign::Plus, &data);
        let data_block = DataBlock(i, block);
        data_blocks.push(data_block);
    }

    let ee = ErasureEntity::load_from_blocks(data_blocks).unwrap();
    // let printed = format!("{ee:?}");
    // fs::write("/tmp/matrix", printed).unwrap();
    println!("*** *** *** *** *** ***");

    let erasure_blocks = ee.gen_erasure_file(2);
    for eb in erasure_blocks {
        let order = eb.0;
        let file_name = format!("src/test_pics/origin/{order}.png");
        let (_, data) = eb.1.to_bytes_be();
        fs::write(file_name, data).unwrap();
    }

    for i in 0..6 {
        let data_block = ee.calc_data(i);
        let order = data_block.0;
        let file_name = format!("src/test_pics/calced/{order}.png");
        let (_, data) = data_block.1.to_bytes_be();
        fs::write(file_name, data).unwrap();
    }
}
