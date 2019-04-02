extern crate libc;

use libc::{c_int, c_uint, uint8_t};

extern "C" {
    fn xd3_encode_memory(
        input: *const uint8_t,
        input_size: c_uint,
        source: *const uint8_t,
        source_size: c_uint,
        output_buffer: *mut uint8_t,
        output_size: *mut c_uint,
        avail_output: c_uint,
        flags: c_int,
    ) -> c_int;
    pub fn xd3_decode_memory(
        input: *const uint8_t,
        input_size: c_uint,
        source: *const uint8_t,
        source_size: c_uint,
        output_buffer: *mut uint8_t,
        output_size: *mut c_uint,
        avail_output: c_uint,
        flags: c_int,
    ) -> c_int;
}

// input: after patch; src: original
pub fn encode(input: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input_len = input.len() as c_uint;
        let src_len = src.len() as c_uint;
        let estimated_out_len = (input_len + src_len) + 20;
        let mut avail_output = 0 as c_uint;
        let mut output = Vec::with_capacity(estimated_out_len as usize);
        let result = xd3_encode_memory(
            input.as_ptr(),
            input_len,
            src.as_ptr(),
            src_len,
            output.as_mut_ptr(),
            &mut avail_output,
            estimated_out_len,
            0,
        );
        if result == 0 {
            output.set_len(avail_output as usize);
            Some(output)
        } else {
            None
        }
    }
}

// input: patch stream; src: to be patched
pub fn decode(input: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input_len = input.len() as c_uint;
        let src_len = src.len() as c_uint;
        let estimated_out_len = (input_len + src_len) + 20;
        let mut avail_output = 0 as c_uint;
        let mut output = Vec::with_capacity(estimated_out_len as usize);
        let result = xd3_decode_memory(
            input.as_ptr(),
            input_len,
            src.as_ptr(),
            src_len,
            output.as_mut_ptr(),
            &mut avail_output,
            estimated_out_len,
            0,
        );
        if result == 0 {
            output.set_len(avail_output as usize);
            Some(output)
        } else {
            None
        }
    }
}
