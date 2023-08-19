use ndarray::prelude::*;
use num_bigint::BigInt;

static PRECISE_CTRL: usize = 65536;

#[derive(Debug)]
pub struct DataBlock(pub usize, pub BigInt);

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
        for block in blocks {
            // first push data, then ascend order
            let curr_order = block.0;
            let row_vector = build_vector_with_data(curr_order, max_order, block.1);
            matrix_raw_vec.extend(row_vector);
        }
        let matrix = Array2::from_shape_vec(shape, matrix_raw_vec)?;
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
