#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use xdelta3::*;

    fn decode2(input: &[u8], src: &[u8]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        futures::executor::block_on(decode_async(input, src, &mut out)).expect("failed to decode");
        Some(out)
    }

    fn check_decode(input: &[u8], src: &[u8]) -> Vec<u8> {
        let out_mem = decode(input, src).expect("Failed to decode");
        let out_async = decode2(input, src).expect("Failed to decode");

        assert_eq!(out_mem, out_async);
        out_mem
    }

    #[test]
    fn basic_recoding() {
        let result =
            encode(&[1, 2, 3, 4, 5, 6, 7], &[1, 2, 4, 4, 7, 6, 7]).expect("failed to encode");
        let recode = check_decode(&result, &[1, 2, 4, 4, 7, 6, 7]);
        assert_eq!(&recode, &[1, 2, 3, 4, 5, 6, 7]);
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

        let patched_data = check_decode(&patch_data, &original_data);
        assert_eq!(patched_data, correct_data);
    }
}
