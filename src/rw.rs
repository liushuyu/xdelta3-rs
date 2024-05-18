/*
use std::pin::Pin;
use std::task::{Context, Poll};

struct Xdelta3Reader<R1, R2> {
    input: R1,
    state: ProcessState<R2>,
    mode: Mode,
}

impl<R1, R2> AsyncRead for Xdelta3Reader<R1, R2>
where
    R1: Unpin + AsyncRead,
    R2: Unpin + AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize>> {
        use binding::xd3_rvalues::*;

        let mode = self.as_ref().mode;
        let state = &mut self.as_mut().state;

        loop {
            match state.step(mode) {
                XD3_INPUT => {
                    if state.eof {
                        break;
                    }
                    // state.read_input(&mut input).await?;
                }
                XD3_OUTPUT => {
                    // state.write_output(&mut output).await?;
                }
                XD3_GETSRCBLK => {
                    // state.src_buf.getblk().await;
                }
                XD3_GOTHEADER | XD3_WINSTART | XD3_WINFINISH => {
                    // do nothing
                }
                XD3_TOOFARBACK | XD3_INTERNAL | XD3_INVALID | XD3_INVALID_INPUT | XD3_NOSECOND
                | XD3_UNIMPLEMENTED => {
                    return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other)));
                }
            }
        }

        unimplemented!();
    }
}
*/
