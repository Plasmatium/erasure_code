mod cli;
mod data_block;
mod manager;

use std::fs;

use clap::Parser;
use manager::{Manager, PartsParam};
use tracing::error;

use crate::cli::Cli;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Cli::parse();
    match args.command {
        cli::Commands::Create {
            file_name,
            data_dir,
            ref pattern,
        } => {
            let parts_params: PartsParam = pattern.parse()?;
            Manager::split_file_to_parts(&file_name, parts_params.0, &data_dir)?;
            let mgr = Manager::new(parts_params, &data_dir)?;
            mgr.reconstruct_parts()?
        }
        cli::Commands::Rebuild {
            data_dir,
            output_file_name,
            force,
        } => {
            let exists = fs::metadata(&output_file_name).is_ok();
            if exists && !force {
                error!("file: '{output_file_name:?}' exists, use --force to overwrite");
                return Ok(());
            }
            let md_file_path = data_dir.join("meta.json");
            let mgr = Manager::load_from_meta(&md_file_path)?;
            let (data_parts, _) = mgr.get_parts_params();
            mgr.reconstruct_parts()?;
            Manager::merge_parts_to_file(&output_file_name, &data_dir, data_parts)?
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use ndarray::Array2;
    use num_bigint::{BigInt, Sign};
    use num_rational::BigRational;

    type MyBR = Rc<RefCell<BigRational>>;

    // test for big rational
    #[test]
    fn test_big_rational1() {
        let a = vec![1u8, 3, 45, 3, 254];
        let big1 = BigInt::from_bytes_le(Sign::Plus, &a);
        let big2 = &big1 * 12;
        let big_rational = BigRational::new(big1, 1.into());
        println!("{big_rational}");
        let numer = big_rational.numer();
        let denom = big_rational.denom();
        println!("{numer}, {denom}");
        let (sign, rebuild) = numer.to_bytes_le();
        println!("{sign:?}, {rebuild:?}")
    }

    #[test]
    fn test_rc_big_rational() {
        let a = BigInt::from_bytes_le(Sign::Plus, &vec![1u8, 3, 45, 3, 254]);
        let b = BigInt::from_bytes_le(Sign::Plus, &vec![32u8, 65, 32, 6, 91, 44, 113]);
        let br1 = BigRational::new(a.clone(), 234125.into());
        let br2 = BigRational::new(b.clone(), a.clone());
        let br3 = BigRational::new(a.clone(), b.clone());
        let br4 = BigRational::new(b.clone() + a.clone(), b.clone() * 324342111 - a.clone());

        let br1 = new_br(br1);
        let br2 = new_br(br2);
        let br3 = new_br(br3);
        let br4 = new_br(br4);

        let arr: Array2<MyBR> = Array2::from_shape_vec((2, 2), vec![br1, br2, br3, br4]).unwrap();
    }

    fn new_br(br: BigRational) -> MyBR {
        Rc::new(RefCell::new(br))
    }
}
