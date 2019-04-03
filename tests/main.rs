#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use xdelta3::{decode, encode};

    #[test]
    fn basic_recoding() {
        let result = encode(&[1, 2, 3, 4, 5, 6, 7], &[1, 2, 4, 4, 7, 6, 7]);
        let recode = decode(result.unwrap().as_slice(), &[1, 2, 4, 4, 7, 6, 7]);
        assert_eq!(recode.unwrap().as_slice(), &[1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn xdelta_own_test() {
        let fixure_path = "xdelta3/xdelta3/examples/iOS/xdelta3-ios-test/xdelta3-ios-test/";
        let original_fixure = format!("{}/{}", fixure_path, "file_v1.bin");
        let correct_fixure = format!("{}/{}", fixure_path, "file_v2.bin");
        let patch_fixure = format!("{}/{}", fixure_path, "file_v1_to_v2.bin");
        let mut original_fixure = File::open(original_fixure).expect("Failed to open file");
        let mut original_data = Vec::new();
        let mut patch_fixure = File::open(patch_fixure).expect("Failed to open patch");
        let mut patch_data = Vec::new();
        let mut correct_fixure = File::open(correct_fixure).expect("Failed to open reference");
        let mut correct_data = Vec::new();
        original_fixure
            .read_to_end(&mut original_data)
            .expect("Failed to read file");
        patch_fixure
            .read_to_end(&mut patch_data)
            .expect("Failed to read file");
        correct_fixure
            .read_to_end(&mut correct_data)
            .expect("Failed to read file");
        let patched_data = decode(patch_data.as_slice(), original_data.as_slice()).unwrap();
        assert_eq!(patched_data, correct_data);
    }

}
