use decoders::RawImage;
use imageops::*;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct OpToLab {
  cam_to_xyz: [[f32;4];3],
}

impl OpToLab {
  pub fn new(img: &RawImage) -> OpToLab {
    OpToLab{
      cam_to_xyz: img.cam_to_xyz(),
    }
  }
}

impl<'a> ImageOp<'a> for OpToLab {
  fn name(&self) -> &str {"to_lab"}
  fn run(&self, pipeline: &mut PipelineGlobals, inid: BufHash, outid: BufHash) {
    let buf = pipeline.cache.get(&inid).unwrap();
    let cmatrix = self.cam_to_xyz;

    let buf = buf.process_into_new(3, &(|outb: &mut [f32], inb: &[f32]| {
      for (pixin, pixout) in inb.chunks(4).zip(outb.chunks_mut(3)) {
        let r = pixin[0];
        let g = pixin[1];
        let b = pixin[2];
        let e = pixin[3];

        let x = r * cmatrix[0][0] + g * cmatrix[0][1] + b * cmatrix[0][2] + e * cmatrix[0][3];
        let y = r * cmatrix[1][0] + g * cmatrix[1][1] + b * cmatrix[1][2] + e * cmatrix[1][3];
        let z = r * cmatrix[2][0] + g * cmatrix[2][1] + b * cmatrix[2][2] + e * cmatrix[2][3];

        let (l,a,b) = xyz_to_lab(x,y,z);

        pixout[0] = l;
        pixout[1] = a;
        pixout[2] = b;
      }
    }));

    pipeline.cache.put(outid, buf, 1);
  }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct OpFromLab {
}

impl OpFromLab {
  pub fn new(_img: &RawImage) -> OpFromLab {
    OpFromLab{}
  }
}

impl<'a> ImageOp<'a> for OpFromLab {
  fn name(&self) -> &str {"from_lab"}
  fn run(&self, pipeline: &mut PipelineGlobals, inid: BufHash, outid: BufHash) {
    let mut buf = (*pipeline.cache.get(&inid).unwrap()).clone();
    let cmatrix = xyz_to_rec709_matrix();

    buf.mutate_lines(&(|line: &mut [f32], _| {
      for pix in line.chunks_mut(3) {
        let l = pix[0];
        let a = pix[1];
        let b = pix[2];

        let (x,y,z) = lab_to_xyz(l,a,b);

        let r = x * cmatrix[0][0] + y * cmatrix[0][1] + z * cmatrix[0][2];
        let g = x * cmatrix[1][0] + y * cmatrix[1][1] + z * cmatrix[1][2];
        let b = x * cmatrix[2][0] + y * cmatrix[2][1] + z * cmatrix[2][2];

        pix[0] = r;
        pix[1] = g;
        pix[2] = b;
      }
    }));

    pipeline.cache.put(outid, buf, 1);
  }
}

fn inverse(inm: [[f32;3];3]) -> [[f32;3];3] {
  let invdet = 1.0 / (
    inm[0][0] * (inm[1][1] * inm[2][2] - inm[2][1] * inm[1][2]) -
    inm[0][1] * (inm[1][0] * inm[2][2] - inm[1][2] * inm[2][0]) +
    inm[0][2] * (inm[1][0] * inm[2][1] - inm[1][1] * inm[2][0])
  );

  let mut out = [[0.0; 3];3];
  out[0][0] =  (inm[1][1]*inm[2][2] - inm[2][1]*inm[1][2]) * invdet;
  out[0][1] = -(inm[0][1]*inm[2][2] - inm[0][2]*inm[2][1]) * invdet;
  out[0][2] =  (inm[0][1]*inm[1][2] - inm[0][2]*inm[1][1]) * invdet;
  out[1][0] = -(inm[1][0]*inm[2][2] - inm[1][2]*inm[2][0]) * invdet;
  out[1][1] =  (inm[0][0]*inm[2][2] - inm[0][2]*inm[2][0]) * invdet;
  out[1][2] = -(inm[0][0]*inm[1][2] - inm[1][0]*inm[0][2]) * invdet;
  out[2][0] =  (inm[1][0]*inm[2][1] - inm[2][0]*inm[1][1]) * invdet;
  out[2][1] = -(inm[0][0]*inm[2][1] - inm[2][0]*inm[0][1]) * invdet;
  out[2][2] =  (inm[0][0]*inm[1][1] - inm[1][0]*inm[0][1]) * invdet;

  out
}

fn xyz_to_rec709_matrix() -> [[f32;3];3] {
  let rgb_to_xyz = [
  // sRGB D65
    [ 0.412453, 0.357580, 0.180423 ],
    [ 0.212671, 0.715160, 0.072169 ],
    [ 0.019334, 0.119193, 0.950227 ],
  ];

  inverse(rgb_to_xyz)
}

fn xyz_to_lab(x: f32, y: f32, z: f32) -> (f32,f32,f32) {
  // D50 White
  let xw = 0.9642; let yw = 1.000; let zw = 0.8249;

  let l = 116.0 * labf(y/yw) - 16.0;
  let a = 500.0 * (labf(x/xw) - labf(y/yw));
  let b = 200.0 * (labf(y/yw) - labf(z/zw));

  (l/100.0,(a+128.0)/256.0,(b+128.0)/256.0)
}

static CBRT_MAXVALS: usize = 1 << 16; // 2^16 should be enough precision
lazy_static! {
  static ref CBRT_LOOKUP: Vec<f32> = {
    let mut lookup: Vec<f32> = vec![0.0; CBRT_MAXVALS+1];
    for i in 0..(CBRT_MAXVALS+1) {
      let v = (i as f32) / (CBRT_MAXVALS as f32);
      lookup[i] = v.cbrt();
    }
    lookup
  };
}

fn labf(val: f32) -> f32 {
  let cutoff = (6.0/29.0)*(6.0/29.0)*(6.0/29.0);
  let multiplier = (1.0/3.0) * (29.0/6.0) * (29.0/6.0);
  let constant = 4.0 / 29.0;

  if val > cutoff {
    if val > 0.0 && val < 1.0 { // use the lookup table
      CBRT_LOOKUP[(val*(CBRT_MAXVALS as f32)) as usize]
    } else {
      val.cbrt()
    }
  } else {
    val * multiplier + constant
  }
}

fn lab_to_xyz(l: f32, a: f32, b: f32) -> (f32,f32,f32) {
  // D50 White
  let xw = 0.9642; let yw = 1.000; let zw = 0.8249;

  let cl = l * 100.0;
  let ca = (a * 256.0) - 128.0;
  let cb = (b * 256.0) - 128.0;

  let x = xw * labinvf((1.0/116.0) * (cl+16.0) + (1.0/500.0) * ca);
  let y = yw * labinvf((1.0/116.0) * (cl+16.0));
  let z = zw * labinvf((1.0/116.0) * (cl+16.0) - (1.0/200.0) * cb);

  (x,y,z)
}

fn labinvf(val: f32) -> f32 {
  let cutoff = 6.0 / 29.0;
  let multiplier = 3.0 * (6.0/29.0) * (6.0/29.0);
  let constant = multiplier * (-4.0 / 29.0);

  if val > cutoff {
    val * val * val
  } else {
    val * multiplier + constant
  }
}
