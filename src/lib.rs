// Copyright 2023 Sung-Cheol Kim. All rights reserved.

use std::error::Error;
use std::time::Instant;
use std::path::Path;

use aws_sdk_s3::Client;
use clap::Parser;
use futures::executor::block_on;
use linya::{Bar, Progress};
use polars::prelude::*;
use plotters::prelude::*;
use ndarray::prelude::*;

#[derive(Parser, Debug)]
#[command(author = "sungcheolkim", version, about, long_about = None)]
/// mouse trajectory viewer
pub struct Config {
    /// how many frames to generate
    #[arg(short, long, default_value_t = 0)]
    frames: usize,

    /// mili-seconds between frames
    #[arg(short, long, default_value_t = 40)]
    secs: u32,

    /// initial pitch 0.2617 or 0.5234
    #[arg(short, long, default_value_t = 0.5234)]
    initial_pitch: f64,

    /// number of skipped frames
    #[arg(short, long, default_value_t = 40)]
    skip: usize,

    /// filekey
    #[arg(short, long, default_value_t = String::from("walker"))]
    filekey: String,

    /// output folder
    #[arg(short, long, default_value_t = String::from("data"))]
    output_dir: String,

    /// input folder
    #[arg(short, long, default_value_t = String::from("input"))]
    input_dir: String,
}


// reading csv file in local drive and search s3
fn load_csv(config: &Config) -> PolarsResult<DataFrame> {
    let start = Instant::now();

    // handle file or s3
    let df_path = Path::new(&config.input_dir).join(format!("{}.csv", config.filekey));
    let df = if df_path.exists() {
        println!("Read from {}", df_path.display());
        CsvReader::from_path(df_path)?.has_header(true).with_comment_char(Some(b'#')).finish()?
    } else {
        // download file from s3
        println!("Download from s3 {}", config.filekey);
        block_on(download_stat(&config.filekey))
    };

    let new_df = df.clone().lazy().select([
        col("x").fill_null(0f64).alias("x"),
        col("y").fill_null(0f64).alias("y"),
        col("z").fill_null(0f64).alias("z"),
        col("t").fill_null(0f64).alias("t"),
    ]).collect()?;

    println!("{:?}", new_df);
    println!("Loading time: {:?}, Length: {}", start.elapsed(), new_df.height());

    Ok(new_df)
}

pub fn run(config: Config) -> Result<(), Box<dyn Error>> {
    // load csv file
    let df = load_csv(&config)?;
    
    // set end frame
    let end_frame = if config.frames > 0 && config.frames < df.height() {
        config.frames
    } else {
        df.height()
    };

    // prepare plot
    let file_path = format!("{}/{}_traj.gif", config.output_dir, config.filekey);
    let area = BitMapBackend::gif(&file_path, (600, 450), config.secs)?
        .into_drawing_area();

    // set view angles
    let mut delta: f64 = -0.002;
    let mut yaw: f64 = 1.05;
    let mut frame: usize = 0;

    // convert to ndarray
    let df_array = df.to_ndarray::<Float64Type>()?;

    // start process
    let start = Instant::now();
    let mut progress = Progress::new();
    let bar: Bar = progress.bar(end_frame, "Image Generation");

    // create frames
    while frame  + 4 * config.skip < end_frame {
        // prepare points
        let points = df_array.slice(s![frame..frame + 4 * config.skip, 0usize..3usize]);
        let t0 = df_array[[frame, 3]];

        // (x, y, z)
        let mut xyz = Vec::new();
        let mut proj_xz = Vec::new();
        let mut proj_yz = Vec::new();
        let mut proj_xy = Vec::new();
        let wall: f64 = if yaw > 0.0 { -1.0  } else { 25.0 };

        for v in points.outer_iter() {
            xyz.push((v[0], v[2], v[1]));
            proj_xy.push((v[0], -1.0f64, v[1]));
            proj_xz.push((v[0], v[2], -1.0f64));
            proj_yz.push((wall, v[2], v[1]));
        }

        // println!("generate frame: {}, time: {:2}, peiod: {}, data len: {}", frame, t0, get_period(t0), points.len());

        area
            .fill(&WHITE)?;
        area
            .draw(&Text::new(format!("period: {}", 0), (20, 400), ("sans-serif", 15.0).into_font()))?;
        area
            .draw(&Text::new(format!("time: {:.2}", t0), (20, 420), ("sans-serif", 15.0).into_font()))?;

        // the coordinate system is (x, z, y)
        let mut chart = ChartBuilder::on(&area)
            .margin(10)
            .caption(&config.filekey, ("sans-serif", 30))
            .build_cartesian_3d(-1.0..25.0, -1.0..20.0, -1.0..25.0)?;

        // change direction 
        delta = match yaw {
            t if t < 0.52 => 0.002,
            t if t > 1.05 => -0.002, 
            _ => delta,
        };
        yaw += delta;

        chart.with_projection(|mut pb| {
            pb.pitch = config.initial_pitch;
            pb.yaw = yaw;
            pb.scale = 0.8;
            pb.into_matrix()
        });

        chart
            .configure_axes()
            //.light_grid_style(BLACK.mix(0.15))
            //.max_light_lines(5)
            .draw()?;
        chart
            .draw_series(LineSeries::new(xyz, BLACK.filled()).point_size(1))?
            .label("Body")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLACK));
        chart
            .draw_series(LineSeries::new(proj_xy, &BLUE))?
            .label("Proj. XY")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));
        chart
            .draw_series(LineSeries::new(proj_xz, &GREEN))?
            .label("Proj. XZ")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &GREEN));
        chart
            .draw_series(LineSeries::new(proj_yz, &RED))?
            .label("Proj. YZ")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));
        chart
            .configure_series_labels()
            .border_style(&BLACK)
            .draw()?;

        area.present().expect("Unable to write result to file!");

        progress.inc_and_draw(&bar, config.skip);
        frame += config.skip;
    }

    println!("Processing Time: {:?}", start.elapsed());
    println!("Save to {}", file_path);

    Ok(())
}

async fn download_stat(filekey: &String) -> DataFrame {
    // create client
    let config = aws_config::from_env().region("us-east-1").load().await;
    let client = Client::new(&config);
    let key = format!("statistics/{}_statistics_1000_82000.csv", filekey);

    let req = client.get_object().bucket("sc-pipeline-output").key(key);

    let res = req.clone().send().await.unwrap();
    let bytes = res.body.collect().await.unwrap();
    let bytes = bytes.into_bytes();

    let cursor = std::io::Cursor::new(bytes);

    let df = CsvReader::new(cursor).finish().unwrap();

    df
}
