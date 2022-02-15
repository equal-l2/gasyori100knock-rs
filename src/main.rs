use std::path::Path;

use anyhow::Result;
use png::OutputInfo;

macro_rules! die {
    ($( $x:expr ),*) => {
        {
            eprintln!($($x,)*);
            std::process::exit(1)
        }
    }
}

struct Args {
    input: String,
    output: String,
    num: usize,
}

#[derive(Clone, Debug)]
struct Info {
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
}

impl From<OutputInfo> for Info {
    fn from(input: OutputInfo) -> Self {
        Self {
            width: input.width,
            height: input.height,
            color: input.color_type,
            depth: input.bit_depth,
        }
    }
}

struct Transformed {
    info: Info,
    bytes: Vec<u8>,
}

fn main() {
    let args = read_args();

    let (info, bytes) =
        read_input(args.input).unwrap_or_else(|e| die!("[ERROR] failed to read input ({})", e));
    println!("[INFO] input read {:?}", info);

    if info.depth != png::BitDepth::Eight {
        die!("[ERROR] the only supported bit depth is 8");
    }

    let funcs: &[fn(Info, Vec<u8>) -> Transformed] = &[
        // identity
        |info, bytes| Transformed { info, bytes },
        // q1
        |info, bytes| {
            // rgb -> bgr
            assert_eq!(info.color, png::ColorType::Rgb);
            let out = bytes
                .chunks(3)
                .filter_map(|chunk| match chunk {
                    [r, g, b] => Some([b, g, r]),
                    _ => None,
                })
                .flatten()
                .copied()
                .collect();
            Transformed { info, bytes: out }
        },
        // q2
        to_grayscale,
        // q3
        |info, bytes| binarize(info, bytes, 128),
    ];

    let trans = funcs
        .get(args.num)
        .unwrap_or_else(|| die!("[ERROR] no function for number {}", args.num));
    let Transformed { info, bytes } = trans(info, bytes);

    write_output(args.output, &info, bytes)
        .unwrap_or_else(|e| die!("[ERROR] failed to write output ({})", e));
    println!("[INFO] wrote output {:?}", info);
}

fn read_input<T: AsRef<Path>>(input: T) -> Result<(Info, Vec<u8>)> {
    let input_handle = std::fs::File::open(input)?;
    let decoder = png::Decoder::new(input_handle);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?.into();
    Ok((info, buf))
}

fn write_output<P, B>(output: P, info: &Info, buf: B) -> Result<()>
where
    P: AsRef<Path>,
    B: AsRef<[u8]>,
{
    let output_handle = std::fs::File::create(output)?;
    let mut encoder = png::Encoder::new(output_handle, info.width, info.height);
    encoder.set_color(info.color);
    encoder.set_depth(info.depth);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(buf.as_ref())?;
    Ok(())
}

fn read_args() -> Args {
    let mut args = std::env::args();
    let my_name = args
        .next()
        .unwrap_or_else(|| die!("[ERROR] args[0] is missing"));
    let args_info = || {
        die!("{} [input] [output] [func number]", my_name);
    };

    let input = args.next().unwrap_or_else(args_info);
    let output = args.next().unwrap_or_else(args_info);
    let num = args
        .next()
        .unwrap_or_else(args_info)
        .parse()
        .unwrap_or_else(|e| die!("[ERROR] failed to parse num ({})", e));
    Args { input, output, num }
}

fn to_grayscale(info: Info, bytes: Vec<u8>) -> Transformed {
    assert_eq!(info.color, png::ColorType::Rgb);
    let out = bytes
        .chunks(3)
        .filter_map(|chunk| match chunk {
            [r, g, b] => Some((0.2126 * *r as f64 + 0.7152 * *g as f64 + 0.0722 * *b as f64) as u8),
            _ => None,
        })
        .collect();
    let info_mod = Info {
        color: png::ColorType::Grayscale,
        ..info
    };
    Transformed {
        info: info_mod,
        bytes: out,
    }
}

fn binarize(info: Info, bytes: Vec<u8>, threshold: u8) -> Transformed {
    let gray = to_grayscale(info, bytes);
    let out = gray
        .bytes
        .into_iter()
        .map(|value| if value < threshold { 0 } else { 255 })
        .collect();
    Transformed {
        info: gray.info,
        bytes: out,
    }
}
