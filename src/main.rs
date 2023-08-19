mod erasure;
mod cli;

use std::fs;

use clap::Parser;
use erasure::FileHandler;
use num_bigint::{BigInt, Sign};
use crate::{erasure::{DataBlock, ErasureEntity}, cli::Cli};


fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    
    let args = Cli::parse();
    match args.command {
        cli::Commands::Create { file_name, data_dir, ref pattern } => {
            let metadata = pattern.parse()?;
            let fh = FileHandler::new(metadata, file_name, data_dir);

            fh.split()?;
        },
        cli::Commands::Rebuild { data_dir, output_file_name } => {
            todo!()
        },
    };

    Ok(())

    // if true {
    //     return
    // }

    // let mut data_blocks = vec![];
    // for i in 0..5 {
    //     let file_name = format!("src/test_pics/origin/{i}.png");
    //     let data = fs::read(file_name).unwrap();
    //     let block = BigInt::from_bytes_le(Sign::Plus, &data);
    //     let data_block = DataBlock(i, block);
    //     data_blocks.push(data_block);
    // }

    // let ee = ErasureEntity::load_from_blocks(data_blocks).unwrap();
    // // let printed = format!("{ee:?}");
    // // fs::write("/tmp/matrix", printed).unwrap();
    // println!("*** *** *** *** *** ***");

    // let erasure_blocks = ee.gen_erasure_file(2);
    // for eb in erasure_blocks {
    //     let order = eb.0;
    //     let file_name = format!("src/test_pics/origin/{order}.png");
    //     let (_, data) = eb.1.to_bytes_le();
    //     fs::write(file_name, data).unwrap();
    // }

    // for i in 0..6 {
    //     let data_block = ee.calc_data(i);
    //     let order = data_block.0;
    //     let file_name = format!("src/test_pics/calced/{order}.png");
    //     let (_, data) = data_block.1.to_bytes_le();
    //     fs::write(file_name, data).unwrap();
    // }
}
