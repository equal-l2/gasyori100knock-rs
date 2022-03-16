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

struct Image {
    info: Info,
    bytes: Vec<u8>,
}

struct HSV {
    h: f64, // [0, 360] // [0, 180]
    s: f64, // [0, 255]
    v: f64, // [0, 255]
}

impl HSV {
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        let colors = [r as f64 / 255., g as f64 / 255., b as f64 / 255.];
        let v = *colors
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let v_min = *colors
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let s = v - v_min;
        let [r, g, b] = colors;
        let h = if s == 0. {
            0.
        } else if b == v_min {
            60. * ((g - r) / s) + 60.
        } else if r == v_min {
            60. * ((b - g) / s) + 180.
        } else if g == v_min {
            60. * ((r - b) / s) + 300.
        } else {
            unreachable!()
        };
        assert!((0.0..360.0).contains(&h));
        assert!((0.0..=1.0).contains(&s));
        assert!((0.0..=1.0).contains(&v));
        Self { h, s, v }
    }

    fn into_rgb(self) -> [u8; 3] {
        let s = self.s;
        let h_prime = self.h / 60.;
        let x = s * (1. - (h_prime % 2. - 1.).abs());
        let z = 0.;
        let rgb_float = match h_prime {
            _ if h_prime < 1. => [s, x, z],
            _ if h_prime < 2. => [x, s, z],
            _ if h_prime < 3. => [z, s, x],
            _ if h_prime < 4. => [z, x, s],
            _ if h_prime < 5. => [x, z, s],
            _ if h_prime < 6. => [s, z, x],
            _ => unreachable!("h_prime should be [0, 6), got {}", h_prime),
        };

        rgb_float.map(|val| {
            let modded = val + (self.v - s);
            assert!(((0.)..=(1.)).contains(&modded));
            (modded * 255.) as u8
        })
    }
}

fn diff<T: PartialOrd + std::ops::Sub<Output = T>>(a: T, b: T) -> T {
    if a > b {
        a - b
    } else {
        b - a
    }
}

fn main() {
    let funcs: &[fn(Image) -> Image] = &[
        // identity
        |img| img,
        // q1
        |img| {
            // rgb -> bgr
            assert_eq!(img.info.color, png::ColorType::Rgb);
            assert!(img.bytes.len() % 3 == 0);
            let out = img.bytes
                .chunks(3)
                .map(|chunk| match chunk {
                    [r, g, b] => [b, g, r],
                    _ => unreachable!(),
                })
                .flatten()
                .copied()
                .collect();
            Image { info: img.info, bytes: out }
        },
        // q2
        to_grayscale,
        // q3
        |img| {
            let img = to_grayscale(img);
            binarize(img, 128)
        },
        // q4
        |img| {
            // Otsu's method
            let gray = to_grayscale(img);
            let histo = {
                let mut bins = [0usize; 256];
                for i in &gray.bytes {
                    bins[*i as usize] += 1;
                }
                bins
            };

            let (best_thres, _) = (0..=255)
                .map(|n| {
                    let sum_l: usize = histo[0..n].into_iter().sum();
                    let sum_r: usize = histo[n..].into_iter().sum();
                    let mulsum_l: usize =
                        histo[0..n].into_iter().zip(0..n).map(|(x, y)| x * y).sum();
                    let mulsum_r: usize = histo[n..255]
                        .into_iter()
                        .zip(n..255)
                        .map(|(x, y)| x * y)
                        .sum();
                    let summul = sum_l * sum_r;
                    if summul != 0 {
                        let dividend = (diff(sum_l * mulsum_r, sum_r * mulsum_l) as f64).powi(2);
                        let res = dividend / summul as f64;
                        Some((n, res))
                    } else {
                        None
                    }
                })
                .filter(Option::is_some)
                .flatten()
                .max_by(|(_, v1), (_, v2)| v1.partial_cmp(v2).expect("encountered NaN"))
                .expect("Failed to find threshold");

            println!("threshold: {}", best_thres);

            binarize(gray, best_thres as u8)
        },
        // q5
        |img| {
            // invert H in HSV
            let mut hsv_bytes = rgb_to_hsv(img.bytes);
            for hsv in &mut hsv_bytes {
                hsv.h = (hsv.h + 180.) % 360.;
            }
            let bytes = hsv_to_rgb(hsv_bytes);
            Image { info: img.info, bytes }
        },
    ];

    let args = read_args();

    let image =
        read_input(args.input).unwrap_or_else(|e| die!("[ERROR] failed to read input ({})", e));
    println!("[INFO] input read {:?}", image.info);

    if image.info.depth != png::BitDepth::Eight {
        die!("[ERROR] the only supported bit depth is 8");
    }


    let trans = funcs
        .get(args.num)
        .unwrap_or_else(|| die!("[ERROR] no function for number {}", args.num));
    let out = trans(image);

    write_output(args.output, &out.info, out.bytes)
        .unwrap_or_else(|e| die!("[ERROR] failed to write output ({})", e));
    println!("[INFO] wrote output {:?}", out.info);
}

fn read_input<T: AsRef<Path>>(input: T) -> Result<Image> {
    let input_handle = std::fs::File::open(input)?;
    let decoder = png::Decoder::new(input_handle);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?.into();
    Ok(Image { info, bytes: buf})
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

fn to_grayscale(img: Image) -> Image {
    assert_eq!(img.info.color, png::ColorType::Rgb);
    assert!(img.bytes.len() % 3 == 0);
    let out = img.bytes
        .chunks(3)
        .filter_map(|chunk| match chunk {
            [r, g, b] => Some((0.2126 * *r as f64 + 0.7152 * *g as f64 + 0.0722 * *b as f64) as u8),
            _ => None,
        })
        .collect();
    let info_mod = Info {
        color: png::ColorType::Grayscale,
        ..img.info
    };
    Image {
        info: info_mod,
        bytes: out,
    }
}

fn binarize(img: Image, threshold: u8) -> Image {
    assert_eq!(img.info.color, png::ColorType::Grayscale);
    let out = img.bytes
        .into_iter()
        .map(|value| if value < threshold { 0 } else { 255 })
        .collect();
    Image { info: img.info, bytes: out }
}

fn rgb_to_hsv(rgb_bytes: Vec<u8>) -> Vec<HSV> {
    assert!(rgb_bytes.len() % 3 == 0);
    rgb_bytes
        .chunks(3)
        .map(|chunk| match chunk {
            [r, g, b] => HSV::from_rgb(*r, *g, *b),
            _ => unreachable!(),
        })
        .collect()
}

fn hsv_to_rgb(hsvs: Vec<HSV>) -> Vec<u8> {
    hsvs.into_iter().map(HSV::into_rgb).flatten().collect()
}
