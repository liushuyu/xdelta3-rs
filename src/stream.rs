use futures_io::*;
use futures_util::io::*;
use std::io;
use std::ops::Range;

use super::binding;
use log::debug;

const XD3_DEFAULT_WINSIZE: usize = 1 << 23;
const XD3_DEFAULT_SRCWINSZ: usize = 1 << 26;
#[allow(unused)]
const XD3_DEFAULT_ALLOCSIZE: usize = 1 << 14;
const XD3_DEFAULT_SPREVSZ: usize = 1 << 18;

struct CacheEntry {
    len: usize,
    buf: Box<[u8]>,
}

struct SrcBuffer<R> {
    src: Box<binding::xd3_source>,
    read: R,
    read_len: usize,
    eof_known: bool,

    block_offset: usize,
    block_len: usize,
    cache: BTreeMap<usize, CacheEntry>,
}

impl<R: AsyncRead + Unpin> SrcBuffer<R> {
    fn new(read: R) -> io::Result<Self> {
        let block_count = 32;
        let max_winsize = XD3_DEFAULT_SRCWINSZ;
        let blksize = max_winsize / block_count;

        let cache = BTreeMap::new();

        let mut src: Box<binding::xd3_source> = Box::new(unsafe { std::mem::zeroed() });
        src.blksize = blksize as u32;
        src.max_winsize = max_winsize as u64;

        Ok(Self {
            src,
            read,
            read_len: 0,
            eof_known: false,

            block_offset: 0,
            block_len: blksize,
            cache,
        })
    }

    async fn fetch(&mut self) -> Result<()> {
        let mut buf = if self.cache.len() == self.block_offset + 1 {
            let mut key = 0usize;
            for (k, _v) in &self.cache {
                key = *k;
                break;
            }
            self.cache.remove(&key).unwrap().buf
        } else {
            let mut v = Vec::with_capacity(self.block_len);
            v.resize(self.block_len, 0);
            v.into_boxed_slice()
        };

        let len = self.read.read(&mut buf).await?;
        if len == 0 {
            self.eof_known = true;
        }
        let entry = CacheEntry { len, buf };
        self.cache.insert(self.block_offset, entry);
        self.block_offset += 1;
        Ok(())
    }

    async fn getblk(&mut self) -> io::Result<()> {
        debug!(
            "getsrcblk: curblkno={}, getblkno={}",
            self.src.curblkno, self.src.getblkno,
        );

        let blkno = self.src.getblkno as usize;

        let entry = loop {
            match self.cache.get_mut(&blkno) {
                Some(entry) => break entry,
                None => {
                    if blkno < self.block_offset {
                        eprintln!("invalid blkno={}, offset={}", blkno, self.block_offset);
                        for (k, _v) in &self.cache {
                            eprintln!("key={:?}", k);
                        }
                        panic!("invalid blkno");
                    }

                    self.fetch().await?;
                    continue;
                }
            }
        };

        let src = &mut self.src;
        let buf_len = entry.len;
        let data = &entry.buf[..buf_len];

        src.curblkno = src.getblkno;
        src.curblk = data.as_ptr();
        src.onblk = buf_len as u32;

        src.eof_known = self.eof_known as i32;
        if !self.eof_known {
            src.max_blkno = src.curblkno;
            src.onlastblk = src.onblk;
        } else {
            src.max_blkno = (self.block_offset - 1) as u64;
            src.onlastblk = (self.read_len % src.blksize as usize) as u32;
        }
        Ok(())
    }
}

struct Xd3Config {
    inner: Box<binding::xd3_config>,
}

impl Xd3Config {
    pub fn new() -> Self {
        let mut cfg: binding::xd3_config = unsafe { std::mem::zeroed() };
        cfg.winsize = XD3_DEFAULT_WINSIZE as u32;
        cfg.sprevsz = XD3_DEFAULT_SPREVSZ as u32;

        let config = Self {
            inner: Box::new(cfg),
        };
        config
    }

    #[allow(unused)]
    fn set_level(&mut self, mut level: i32) {
        use binding::xd3_flags::*;

        if level < 0 {
            level = 0;
        }
        if level > 9 {
            level = 9;
        }
        let flags = (self.inner.flags & (!(XD3_COMPLEVEL_MASK as i32)))
            | (level << XD3_COMPLEVEL_SHIFT as i32);

        self.inner.flags = flags;
    }
}

struct Xd3Stream {
    inner: Box<binding::xd3_stream>,
}
impl Xd3Stream {
    fn new() -> Self {
        let inner: binding::xd3_stream = unsafe { std::mem::zeroed() };
        return Self {
            inner: Box::new(inner),
        };
    }
}
impl Drop for Xd3Stream {
    fn drop(&mut self) {
        unsafe {
            binding::xd3_free_stream(self.inner.as_mut() as *mut _);
        }
    }
}

pub async fn decode_async<R1, R2, W>(input: R1, src: R2, out: W) -> io::Result<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    process_async(Mode::Decode, input, src, out).await
}

pub async fn encode_async<R1, R2, W>(input: R1, src: R2, out: W) -> io::Result<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    process_async(Mode::Encode, input, src, out).await
}

#[derive(Clone, Copy)]
enum Mode {
    Encode,
    Decode,
}

async fn process_async<R1, R2, W>(
    mode: Mode,
    mut input: R1,
    src: R2,
    mut output: W,
) -> io::Result<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let cfg = Xd3Config::new();
    let mut state = ProcessState::new(cfg, src)?;

    use binding::xd3_rvalues::*;

    loop {
        let res = state.step(mode);
        match res {
            XD3_INPUT => {
                if state.eof {
                    break;
                }
                state.read_input(&mut input).await?;
            }
            XD3_OUTPUT => {
                state.write_output(&mut output).await?;
            }
            XD3_GETSRCBLK => {
                state.src_buf.getblk().await?;
            }
            XD3_GOTHEADER | XD3_WINSTART | XD3_WINFINISH => {
                // do nothing
            }
            XD3_TOOFARBACK | XD3_INTERNAL | XD3_INVALID | XD3_INVALID_INPUT | XD3_NOSECOND
            | XD3_UNIMPLEMENTED => {
                return Err(io::Error::new(io::ErrorKind::Other, format!("{:?}", res)));
            }
        }
    }

    output.flush().await
}

struct ProcessState<R> {
    #[allow(unused)]
    cfg: Xd3Config,
    stream: Xd3Stream,
    src_buf: SrcBuffer<R>,
    input_buf: Vec<u8>,
    eof: bool,
}

impl<R> ProcessState<R>
where
    R: AsyncRead + Unpin,
{
    fn new(mut cfg: Xd3Config, src: R) -> io::Result<Self> {
        let mut stream = Xd3Stream::new();
        let stream0 = stream.inner.as_mut();

        let cfg0 = cfg.inner.as_mut();

        let mut src_buf = SrcBuffer::new(src)?;

        let ret = unsafe { binding::xd3_config_stream(stream0, cfg0) };
        if ret != 0 {
            let err = if stream0.msg == std::ptr::null() {
                Error::new(io::ErrorKind::Other, "xd3_config_stream: null")
            } else {
                let msg = unsafe { std::ffi::CStr::from_ptr(stream0.msg) };

                Error::new(
                    io::ErrorKind::Other,
                    format!("xd3_config_stream: {:?}, flags={:0b}", msg, stream0.flags),
                )
            };
            return Err(err);
        }
        log::info!("config={:?}", cfg0);

        let ret = unsafe { binding::xd3_set_source(stream0, src_buf.src.as_mut()) };
        if ret != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "xd3_set_source"));
        }

        let input_buf_size = stream0.winsize as usize;
        debug!("stream.winsize={}", input_buf_size);
        let mut input_buf = Vec::with_capacity(input_buf_size);
        input_buf.resize(input_buf_size, 0u8);

        Ok(Self {
            cfg,
            stream,
            src_buf,
            input_buf,
            eof: false,
        })
    }

    fn step(&mut self, mode: Mode) -> binding::xd3_rvalues {
        unsafe {
            let stream = self.stream.inner.as_mut();
            std::mem::transmute(match mode {
                Mode::Encode => binding::xd3_encode_input(stream),
                Mode::Decode => binding::xd3_decode_input(stream),
            })
        }
    }

    async fn read_input<R2>(&mut self, mut input: R2) -> io::Result<()>
    where
        R2: Unpin + AsyncRead,
    {
        let input_buf = &mut self.input_buf;
        let stream = self.stream.inner.as_mut();

        let read_size = match input.read(input_buf).await {
            Ok(n) => n,
            Err(_e) => {
                debug!("error on read: {:?}", _e);
                return Err(io::Error::new(io::ErrorKind::Other, "xd3: read_input"));
            }
        };

        if read_size == 0 {
            // xd3_set_flags
            stream.flags |= binding::xd3_flags::XD3_FLUSH as i32;
            self.eof = true;
        }

        // xd3_avail_input
        stream.next_in = input_buf.as_ptr();
        stream.avail_in = read_size as u32;
        Ok(())
    }

    async fn write_output<W>(&mut self, mut output: W) -> io::Result<()>
    where
        W: Unpin + AsyncWrite,
    {
        let stream = self.stream.inner.as_mut();

        let out_data =
            unsafe { std::slice::from_raw_parts(stream.next_out, stream.avail_out as usize) };
        output.write_all(out_data).await?;

        // xd3_consume_output
        stream.avail_out = 0;
        Ok(())
    }
}
