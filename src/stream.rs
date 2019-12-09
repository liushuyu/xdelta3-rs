use futures_io::*;
use futures_util::io::*;
use std::ops::Range;

use super::binding;
use log::debug;

#[allow(unused)]
const XD3_DEFAULT_WINSIZE: usize = 1 << 23;
const XD3_DEFAULT_SRCWINSZ: usize = 1 << 26;

struct SrcBuffer<R> {
    src: binding::xd3_source,
    read: R,
    read_len: usize,
    eof_known: bool,

    block_count: usize,
    block_offset: usize,
    buf: Box<[u8]>,
}

impl<R: AsyncRead + Unpin> SrcBuffer<R> {
    async fn new(mut read: R) -> Option<Self> {
        let block_count = 64;
        let max_winsize = XD3_DEFAULT_SRCWINSZ;
        let blksize = max_winsize / block_count;

        let mut src: binding::xd3_source = unsafe { std::mem::zeroed() };
        src.blksize = blksize as u32;
        src.max_winsize = max_winsize as u64;

        let mut buf = Vec::with_capacity(max_winsize);
        buf.resize(max_winsize, 0u8);

        let read_len = read.read(&mut buf).await.ok()?;
        debug!("SrcBuffer::new read_len={}", read_len);

        Some(Self {
            src,
            read,
            read_len,
            eof_known: read_len != buf.len(),

            block_count,
            block_offset: 0,
            buf: buf.into_boxed_slice(),
        })
    }

    async fn fetch(&mut self) -> Option<bool> {
        let idx = self.block_offset;
        let r = self.block_range(idx);
        let block = &mut self.buf[r.clone()];
        let read_len = self.read.read(block).await.ok()?;
        debug!(
            "range={:?}, block_len={}, read_len={}",
            r,
            block.len(),
            read_len
        );

        self.block_offset += 1;
        self.read_len += read_len;

        Some(read_len != block.len())
    }

    async fn prepare(&mut self, idx: usize) -> Option<()> {
        while !self.eof_known && idx >= self.block_offset + self.block_count {
            debug!(
                "prepare idx={}, block_offset={}, block_count={}",
                idx, self.block_offset, self.block_count
            );
            let eof = self.fetch().await?;
            if eof {
                debug!("eof");
                self.eof_known = true;
                break;
            }
        }
        Some(())
    }

    fn block_range(&self, idx: usize) -> Range<usize> {
        debug!("idx={}, offset={}", idx, self.block_offset);
        assert!(idx >= self.block_offset && idx < self.block_offset + self.block_count);

        let idx = idx % self.block_count;
        let start = (self.src.blksize as usize) * idx;
        let end = (self.src.blksize as usize) * (idx + 1);

        let start = start.min(self.read_len);
        let end = end.min(self.read_len);

        start..end
    }

    async fn getblk(&mut self) {
        debug!(
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

        src.eof_known = self.eof_known as i32;
        if !self.eof_known {
            src.max_blkno = src.curblkno;
            src.onlastblk = src.onblk;
        } else {
            src.max_blkno = (self.block_offset + self.block_count - 1) as u64;
            src.onlastblk = (self.read_len % src.blksize as usize) as u32;
        }
    }
}

struct Xd3Stream {
    inner: binding::xd3_stream,
}
impl Xd3Stream {
    fn new() -> Self {
        let inner: binding::xd3_stream = unsafe { std::mem::zeroed() };
        return Self { inner };
    }
}
impl Drop for Xd3Stream {
    fn drop(&mut self) {
        unsafe {
            binding::xd3_free_stream(&mut self.inner as *mut _);
        }
    }
}

pub async fn decode_async<R1, R2, W>(input: R1, src: R2, out: W) -> Option<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    process_async(Mode::Decode, input, src, out).await
}

pub async fn encode_async<R1, R2, W>(input: R1, src: R2, out: W) -> Option<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    process_async(Mode::Encode, input, src, out).await
}

enum Mode {
    Encode,
    Decode,
}

async fn process_async<R1, R2, W>(mode: Mode, mut input: R1, src: R2, mut out: W) -> Option<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut stream = Xd3Stream::new();
    let stream = &mut stream.inner;
    let mut cfg: binding::xd3_config = unsafe { std::mem::zeroed() };
    cfg.winsize = XD3_DEFAULT_WINSIZE as u32;

    let mut src_buf = SrcBuffer::new(src).await?;

    let ret = unsafe { binding::xd3_config_stream(stream, &mut cfg) };
    if ret != 0 {
        return None;
    }

    let ret = unsafe { binding::xd3_set_source(stream, &mut src_buf.src) };
    if ret != 0 {
        return None;
    }

    let input_buf_size = stream.winsize as usize;
    debug!("stream.winsize={}", input_buf_size);
    let mut input_buf = Vec::with_capacity(input_buf_size);
    input_buf.resize(input_buf_size, 0u8);
    let mut eof = false;

    'outer: while !eof {
        let read_size = match input.read(&mut input_buf).await {
            Ok(n) => n,
            Err(_e) => {
                debug!("error on read: {:?}", _e);
                return None;
            }
        };
        debug!("read_size={}", read_size);
        if read_size == 0 {
            // xd3_set_flags
            stream.flags = binding::xd3_flags::XD3_FLUSH as i32;
            eof = true;
        }

        // xd3_avail_input
        stream.next_in = input_buf.as_ptr();
        stream.avail_in = read_size as u32;

        'inner: loop {
            let ret: binding::xd3_rvalues = unsafe {
                std::mem::transmute(match mode {
                    Mode::Encode => binding::xd3_encode_input(stream),
                    Mode::Decode => binding::xd3_decode_input(stream),
                })
            };

            if stream.msg != std::ptr::null() {
                debug!("ret={:?}, msg={:?}", ret, unsafe {
                    std::ffi::CStr::from_ptr(stream.msg)
                },);
            } else {
                debug!("ret={:?}", ret,);
            }

            use binding::xd3_rvalues::*;
            match ret {
                XD3_INPUT => {
                    continue 'outer;
                    //
                }
                XD3_OUTPUT => {
                    let mut out_data = unsafe {
                        std::slice::from_raw_parts(stream.next_out, stream.avail_out as usize)
                    };
                    while !out_data.is_empty() {
                        let n = match out.write(out_data).await {
                            Ok(n) => n,
                            Err(_e) => {
                                debug!("error on write: {:?}", _e);
                                return None;
                            }
                        };
                        out_data = &out_data[n..];
                    }

                    // xd3_consume_output
                    stream.avail_out = 0;
                }
                XD3_GETSRCBLK => {
                    src_buf.getblk().await;
                }
                XD3_GOTHEADER | XD3_WINSTART | XD3_WINFINISH => {
                    // do nothing
                }
                XD3_TOOFARBACK | XD3_INTERNAL | XD3_INVALID | XD3_INVALID_INPUT | XD3_NOSECOND
                | XD3_UNIMPLEMENTED => {
                    return None;
                }
            }
        }
    }

    out.flush().await.ok()
}
