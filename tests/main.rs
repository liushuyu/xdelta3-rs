#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use xdelta3::*;

    fn encode2(input: &[u8], src: &[u8]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        futures::executor::block_on(encode_async(input, src, &mut out)).expect("failed to decode");
        Some(out)
    }

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

    fn read_file(filename: &str) -> Vec<u8> {
        let mut file = File::open(filename).expect("Failed to open file");
        let mut data = Vec::new();

        file.read_to_end(&mut data).expect("Failed to read file");

        data
    }

    #[test]
    fn xdelta_own_test() {
        let fixure_path = "xdelta3/xdelta3/examples/iOS/xdelta3-ios-test/xdelta3-ios-test/";
        let original_data = read_file(&format!("{}/{}", fixure_path, "file_v1.bin"));
        let correct_data = read_file(&format!("{}/{}", fixure_path, "file_v2.bin"));
        let patch_data = read_file(&format!("{}/{}", fixure_path, "file_v1_to_v2.bin"));

        let patched_data = check_decode(&patch_data, &original_data);
        assert_eq!(patched_data, correct_data);
    }

    #[test]
    fn round_trip_test() {
        let fixure_path = "xdelta3/xdelta3/examples/iOS/xdelta3-ios-test/xdelta3-ios-test/";
        let source = read_file(&format!("{}/{}", fixure_path, "file_v1.bin"));
        let input = read_file(&format!("{}/{}", fixure_path, "file_v2.bin"));

        let patch_sync = encode(&input, &source).expect("failed to encode");
        assert_eq!(input, check_decode(&patch_sync, &source));

        let patch_async = encode2(&input, &source).expect("failed to encode");
        assert_eq!(input, check_decode(&patch_async, &source));
    }
}
