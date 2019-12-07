//! # xdelta3
//!
//! This crate is a Rust binding of `xdelta3` which is written in C.
//!
//! In case you are not familar, `xdelta3` implements VCDIFF standard, which is a standard for
//! binary patches.
//! You can generate and apply VCDIFF patch for two similar (but large) binary files quickly using `xdelta3`.
//!
//! The original `xdelta3` utility is widely used for delivering software updates and ROM hacks.
//!
//! You can find out how to use this crate in this documentation and you can also consult the tests in the `test/` folder
//! to see it in action (how to generate and patch two files!)

extern crate libc;

use libc::c_uint;

#[allow(dead_code)]
mod binding {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

/// Function to generate the difference data
///
/// This function is used to generate the difference data.
/// The data in `src` will be treated as "original" data and the
/// data in `input` will be treated as "after", "patched" or "expected" data.
///
/// If you want to build an application that applies patches or differential updates,
/// this function is used to generate the patch data (or update files).
/// When generating the patch file, you might want to read your old file into a `&[u8]` and
/// pass that variable to the `src` parameter and read your new file into another `&[u8]` and
/// pass that variable to the `input` parameter. And then you could write the output of this function
/// to a file.
///
/// Here is a basic example to show how to use this function:
/// ```
/// extern crate xdelta3;
/// use xdelta3::encode;
///
/// fn main() {
///     let result = encode(&[1, 2, 3, 4, 5, 6, 7], &[1, 2, 4, 4, 7, 6, 7]);
///     assert_eq!(result.unwrap().as_slice(), &[214, 195, 196, 0, 0, 0, 13, 7, 0, 7, 1, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
/// }
/// ```
///
/// You might notice the generated patch data is larger than both orginal data and the updated data.
/// But don't worry, if your data is large enough and kind of similar between each other (usually the case
/// for software updates or ROM patches), the patch data should be only a fraction of your updated file.
pub fn encode(input: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input_len = input.len() as c_uint;
        let src_len = src.len() as c_uint;
        let estimated_out_len = (input_len + src_len) * 2;
        let mut avail_output = 0 as c_uint;
        let mut output = Vec::with_capacity(estimated_out_len as usize);
        let result = binding::xd3_encode_memory(
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

/// Function to decode the difference data
///
/// This function is used to decode the difference data.
/// The data in `src` will be treated as "original" data and the
/// data in `input` will be treated as "difference" or "patch" data.
/// The returned `Vec` stores the data that has been patched
///
/// As opposed to the encode function, if you are building an application that
/// applies patches or differential updates, this function should be used to
/// patch or update the old file from the patch data. It's recommeded to check
/// for the file integrity after doing the decode to prevent from creating
/// potentially corrupted files
///
/// Here is a basic example to show how to use this function:
/// ```
/// extern crate xdelta3;
/// use xdelta3::decode;
///
/// fn main() {
///     let result = decode(&[214, 195, 196, 0, 0, 0, 13, 7, 0, 7, 1, 0, 1, 2, 3, 4, 5, 6, 7, 8], &[1, 2, 4, 4, 7, 6, 7]);
///     assert_eq!(result.unwrap().as_slice(), &[1, 2, 3, 4, 5, 6, 7]);
/// }
/// ```
pub fn decode(input: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input_len = input.len() as c_uint;
        let src_len = src.len() as c_uint;
        let estimated_out_len = (input_len + src_len) * 2;
        let mut avail_output = 0 as c_uint;
        let mut output = Vec::with_capacity(estimated_out_len as usize);
        let result = binding::xd3_decode_memory(
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

use futures::io::*;
use std::ops::Range;

pub fn decode2(input: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    futures::executor::block_on(decode_async(input, src, &mut out));
    Some(out)
}

struct SrcBuffer<R> {
    src: binding::xd3_source,
    read: R,
    read_len: usize,
    eof_known: bool,

    block_count: usize,
    block_start: usize,
    block_offset: usize,
    buf: Box<[u8]>,
}

impl<R: AsyncRead + Unpin> SrcBuffer<R> {
    async fn new(mut read: R) -> Self {
        let block_count = 16;
        let blksize = 32768;
        let max_winsize = blksize * block_count;

        let mut src: binding::xd3_source = unsafe { std::mem::zeroed() };
        src.blksize = blksize as u32;
        src.max_winsize = max_winsize as u64;

        let mut buf = Vec::with_capacity(max_winsize);
        buf.resize(max_winsize, 0u8);

        let read_len = read.read(&mut buf).await.unwrap();

        Self {
            src,
            read,
            read_len,
            eof_known: read_len != buf.len(),

            block_count,
            block_start: 0,
            block_offset: 0,
            buf: buf.into_boxed_slice(),
        }
    }

    async fn fetch(&mut self) -> bool {
        self.block_start = (self.block_start + 1) % self.block_count;
        self.block_offset += 1;

        let idx = self.block_start + self.block_count - 1;
        let r = self.block_range(idx);
        let block = &mut self.buf[r];
        let read_len = self.read.read(block).await.unwrap();

        self.read_len += read_len;

        read_len != block.len()
    }

    async fn prepare(&mut self, idx: usize) {
        while idx >= self.block_start + self.block_count {
            let eof = self.fetch().await;
            if eof {
                self.eof_known = true;
                break;
            }
        }
    }

    fn block_range(&self, idx: usize) -> Range<usize> {
        assert!(idx >= self.block_start);

        let idx = (idx + self.block_start) % self.block_count;
        let start = (self.src.blksize as usize) * idx;
        let end = (self.src.blksize as usize) * (idx + 1);

        let start = start.min(self.read_len);
        let end = end.min(self.read_len);

        start..end
    }

    async fn getblk(&mut self) {
        println!(
            "getsrcblk: curblkno={}, getblkno={}",
            self.src.curblkno, self.src.getblkno,
        );

        let blkno = self.src.getblkno as usize;
        self.prepare(blkno).await;
        let range = self.block_range(blkno);

        let src = &mut self.src;
        let data = &self.buf[range];

        src.curblkno = src.getblkno;
        src.curblk = data.as_ptr();
        src.onblk = data.len() as u32;

        src.max_blkno = src.curblkno;
        src.onlastblk = src.onblk;
        src.eof_known = self.eof_known as i32;
    }
}

pub async fn decode_async<R1, R2, W>(mut input: R1, src: R2, mut out: W)
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut stream: binding::xd3_stream = unsafe { std::mem::zeroed() };
    let mut cfg: binding::xd3_config = unsafe { std::mem::zeroed() };

    let mut src_buf = SrcBuffer::new(src).await;

    let ret = unsafe { binding::xd3_config_stream(&mut stream, &mut cfg) };
    assert_eq!(ret, 0);

    let ret = unsafe { binding::xd3_set_source(&mut stream, &mut src_buf.src) };
    assert_eq!(ret, 0);

    let mut input_buf = [0u8; 1024 * 4];

    'outer: loop {
        let read_size = input.read(&mut input_buf).await.unwrap();
        println!("read_size={}", read_size);
        if read_size == 0 {
            break;
        }

        // xd3_avail_input
        stream.next_in = input_buf.as_ptr();
        stream.avail_in = read_size as u32;
        // xd3_set_flags
        if read_size != input_buf.len() {
            stream.flags = binding::xd3_flags::XD3_FLUSH as i32;
        }

        loop {
            let ret: binding::xd3_rvalues =
                unsafe { std::mem::transmute(binding::xd3_decode_input(&mut stream)) };

            if stream.msg != std::ptr::null() {
                println!(
                    "msg={:?}, ret={:?}",
                    unsafe { std::ffi::CStr::from_ptr(stream.msg) },
                    ret
                );
            }

            use binding::xd3_rvalues::*;
            match ret {
                XD3_INPUT => {
                    println!("input");
                    continue 'outer;
                    //
                }
                XD3_OUTPUT => {
                    println!("output");
                    let out_data = unsafe {
                        std::slice::from_raw_parts(stream.next_out, stream.avail_out as usize)
                    };
                    out.write(out_data).await.unwrap();
                }
                XD3_GETSRCBLK => {
                    src_buf.getblk().await;
                }
                XD3_GOTHEADER | XD3_WINSTART | XD3_WINFINISH => {
                    //
                }
                XD3_TOOFARBACK | XD3_INTERNAL | XD3_INVALID | XD3_INVALID_INPUT | XD3_NOSECOND
                | XD3_UNIMPLEMENTED => {
                    unimplemented!();
                    //
                }
            }
        }
    }
}
