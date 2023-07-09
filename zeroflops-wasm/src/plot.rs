use plotters::prelude::{
    ChartBuilder, Circle, Color, Histogram, IntoDrawingArea, IntoSegmentedCoord, LineSeries, BLACK,
    RED, WHITE,
};
use plotters_canvas::CanvasBackend;
use polars::prelude::{DataFrame, DataType};
use std::collections::HashMap;

pub enum DataView {
    Table,
    ColumnGraph,
    LineGraph,
    ScatterPlot,
    CumLineGraph,
}

impl DataView {
    pub fn draw(&self, df: &DataFrame) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            DataView::Table => Ok(()),
            DataView::ColumnGraph => draw_column_graph(df),
            DataView::LineGraph => draw_line_graph(df),
            DataView::ScatterPlot => draw_scatter_plot(df),
            DataView::CumLineGraph => draw_cum_line_graph(df),
        }
    }
}

fn draw_column_graph(df: &DataFrame) -> Result<(), Box<dyn std::error::Error>> {
    let backend = CanvasBackend::new("canvas").expect("cannot find canvas");
    let root = backend.into_drawing_area();

    root.fill(&WHITE)?;

    let mut builder = ChartBuilder::on(&root);
    builder
        .x_label_area_size(35)
        .y_label_area_size(40)
        .margin(5);
    let range = df[1].cast(&DataType::Float64)?;
    match df[0].dtype() {
        DataType::Int64 => {
            let mut data = HashMap::new();
            for (i, f) in df[0].i64()?.into_iter().zip(range.f64()?.into_iter()) {
                *data.entry(i.unwrap() as u32).or_insert(0f64) += f.unwrap();
            }
            let domain = 0u32..df[0].max().unwrap();
            let mut chart = builder.build_cartesian_2d(
                domain.into_segmented(),
                0f64..*data.values().max_by(|a, b| a.total_cmp(b)).unwrap(),
            )?;
            chart
                .configure_mesh()
                .disable_x_mesh()
                .bold_line_style(WHITE.mix(0.3))
                .y_desc(&df.fields()[1].name)
                .x_desc(&df.fields()[0].name)
                .axis_desc_style(("sans-serif", 15))
                .draw()?;
            chart.draw_series(
                Histogram::vertical(&chart)
                    .style(RED.mix(0.5).filled())
                    .data(data.into_iter()),
            )?;
        }
        DataType::Utf8 => {
            let data: HashMap<_, _> = df[0]
                .utf8()?
                .into_iter()
                .zip(range.f64()?.into_iter())
                .map(|(o1, o2)| (o1.unwrap(), o2.unwrap()))
                .collect();
            let domain = data.keys().cloned().collect::<Vec<_>>();
            let mut chart = builder.build_cartesian_2d(
                domain.into_segmented(),
                0f64..*data.values().max_by(|a, b| a.total_cmp(b)).unwrap(),
            )?;
            chart
                .configure_mesh()
                .disable_x_mesh()
                .bold_line_style(WHITE.mix(0.3))
                .y_desc(&df.fields()[1].name)
                .x_desc(&df.fields()[0].name)
                .axis_desc_style(("sans-serif", 15))
                .draw()?;
            chart.draw_series(
                Histogram::vertical(&chart)
                    .style(RED.mix(0.5).filled())
                    .data(data.iter().map(|(s, i)| (s, *i))),
            )?;
        }
        _ => todo!(),
    }
    Ok(())
}

fn draw_line_graph(df: &DataFrame) -> Result<(), Box<dyn std::error::Error>> {
    let backend = CanvasBackend::new("canvas").expect("cannot find canvas");
    let root = backend.into_drawing_area();

    root.fill(&WHITE)?;

    let mut builder = ChartBuilder::on(&root);
    builder
        .x_label_area_size(35)
        .y_label_area_size(40)
        .margin(5);
    let domain = df[0].cast(&DataType::Float64)?;
    let range = df[1].cast(&DataType::Float64)?;
    let data: Vec<_> = domain
        .f64()?
        .into_iter()
        .zip(range.f64()?.into_iter())
        .map(|(o1, o2)| (o1.unwrap(), o2.unwrap()))
        .collect();
    let mut chart =
        builder.build_cartesian_2d(0f64..df[0].max().unwrap(), 0f64..df[1].max().unwrap())?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(data.into_iter(), BLACK))?;
    Ok(())
}

fn draw_scatter_plot(df: &DataFrame) -> Result<(), Box<dyn std::error::Error>> {
    let backend = CanvasBackend::new("canvas").expect("cannot find canvas");
    let root = backend.into_drawing_area();

    root.fill(&WHITE)?;

    let mut builder = ChartBuilder::on(&root);
    builder
        .x_label_area_size(35)
        .y_label_area_size(40)
        .margin(5);
    let domain = df[0].cast(&DataType::Float64)?;
    let range = df[1].cast(&DataType::Float64)?;
    let data: Vec<_> = domain
        .f64()?
        .into_iter()
        .zip(range.f64()?.into_iter())
        .map(|(o1, o2)| Circle::new((o1.unwrap(), o2.unwrap()), 2, BLACK.filled()))
        .collect();
    let mut chart =
        builder.build_cartesian_2d(0f64..df[0].max().unwrap(), 0f64..df[1].max().unwrap())?;
    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .draw()?;
    chart.draw_series(data.into_iter())?;
    Ok(())
}

fn draw_cum_line_graph(df: &DataFrame) -> Result<(), Box<dyn std::error::Error>> {
    let backend = CanvasBackend::new("canvas").expect("cannot find canvas");
    let root = backend.into_drawing_area();

    root.fill(&WHITE)?;

    let mut builder = ChartBuilder::on(&root);
    builder
        .x_label_area_size(35)
        .y_label_area_size(40)
        .margin(5);
    let domain = df[0].cast(&DataType::Float64)?;
    let range = df[1].cast(&DataType::Float64)?;
    let mut cum_sum = 0.0;
    let mut data = Vec::new();
    let mut max = 0.0;
    for (d, f) in domain.f64()?.into_iter().zip(range.f64()?.into_iter()) {
        data.push((cum_sum, f.unwrap()));
        cum_sum += d.unwrap();
        data.push((cum_sum, f.unwrap()));
        max = f64::max(max, f.unwrap());
    }
    let mut chart = builder.build_cartesian_2d(0f64..cum_sum, 0f64..max)?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(data.into_iter(), BLACK))?;
    Ok(())
}
