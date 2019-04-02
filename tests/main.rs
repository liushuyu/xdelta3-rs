#[cfg(test)]
mod tests {
    use xdelta3::{decode, encode};

    #[test]
    fn basic_recoding() {
        let result = encode(&[1, 2, 3, 4, 5, 6, 7], &[1, 2, 4, 4, 7, 6, 7]);
        let recode = decode(result.unwrap().as_slice(), &[1, 2, 4, 4, 7, 6, 7]);
        assert_eq!(recode.unwrap().as_slice(), &[1, 2, 3, 4, 5, 6, 7]);
    }

}
