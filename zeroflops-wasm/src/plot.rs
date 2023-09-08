use plotters::prelude::{
    ChartBuilder, Circle, Color, Histogram, IntoDrawingArea, IntoSegmentedCoord, LineSeries, BLACK,
    RED, WHITE,
};
use plotters_canvas::CanvasBackend;
use polars::prelude::{DataFrame, DataType};
use std::collections::HashMap;
use yew::{html, Html};

pub enum DataView {
    Table,
    ColumnGraph,
    LineGraph,
    ScatterPlot,
    CumLineGraph,
}

impl DataView {
    pub fn render(&self, df: &DataFrame) -> Html {
        html! {
            <div>
                <canvas id="canvas" width="640" height="426" class={if let DataView::Table = self { "d-none" } else { "" }}></canvas>
                if let DataView::Table = self {
                    {df_table_view(df)}
                }
            </div>
        }
    }

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

pub fn df_table_view(df: &DataFrame) -> Html {
    html! {
        <div class="table-responsive">
            <table class="table table-striped mb-0">
                <thead>
                    <tr>
                        <th>{"#"}</th>
                        {for df.fields().iter().map(|f| html! {
                            <th>{&f.name()}</th>
                        })}
                    </tr>
                </thead>
                <tbody>{for (0..df.height()).map(|i| df_item_view(df, i))}</tbody>
            </table>
        </div>
    }
}

fn df_item_view(df: &DataFrame, i: usize) -> Html {
    html! {
        <tr>
            <th>{i + 1}</th>
            {for df.iter().map(|item| html! {
                <td class="text-truncate max-width">{item.str_value(i).unwrap()}</td>
            })}
        </tr>
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
        DataType::Int64 | DataType::UInt64 => {
            let mut data = HashMap::new();
            match df[0].dtype() {
                DataType::Int64 => {
                    for (i, f) in df[0].i64()?.into_iter().zip(range.f64()?.into_iter()) {
                        *data.entry(i.unwrap() as u32).or_insert(0f64) += f.unwrap();
                    }
                }
                DataType::UInt64 => {
                    for (i, f) in df[0].u64()?.into_iter().zip(range.f64()?.into_iter()) {
                        *data.entry(i.unwrap() as u32).or_insert(0f64) += f.unwrap();
                    }
                }
                _ => unreachable!(),
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
                .y_desc(&*df.fields()[1].name)
                .x_desc(&*df.fields()[0].name)
                .axis_desc_style(("sans-serif", 15))
                .draw()?;
            chart.draw_series(
                Histogram::vertical(&chart)
                    .style(RED.mix(0.5).filled())
                    .data(data.into_iter()),
            )?;
        }
        DataType::Utf8 => {
            let data: Vec<_> = df[0]
                .utf8()?
                .into_iter()
                .zip(range.f64()?.into_iter())
                .map(|(o1, o2)| (o1.unwrap(), o2.unwrap()))
                .collect();
            let domain = data.iter().map(|(s, _)| s).cloned().collect::<Vec<_>>();
            let mut chart = builder.build_cartesian_2d(
                domain.into_segmented(),
                0f64..*data
                    .iter()
                    .map(|(_, i)| i)
                    .max_by(|a, b| a.total_cmp(b))
                    .unwrap(),
            )?;
            chart
                .configure_mesh()
                .disable_x_mesh()
                .bold_line_style(WHITE.mix(0.3))
                .y_desc(&*df.fields()[1].name)
                .x_desc(&*df.fields()[0].name)
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
    let data = df_coords(df)?;
    let mut chart =
        builder.build_cartesian_2d(0f64..df[0].max().unwrap(), 0f64..df[1].max().unwrap())?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(data, BLACK))?;
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
    let data = df_coords(df)?;
    let mut chart =
        builder.build_cartesian_2d(0f64..df[0].max().unwrap(), 0f64..df[1].max().unwrap())?;
    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .draw()?;
    chart.draw_series(data.into_iter().map(|c| Circle::new(c, 2, BLACK.filled())))?;
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
    let df = df_coords(df)?;
    let mut cum_sum = 0.0;
    let mut data = Vec::with_capacity(2 * df.len());
    let mut max = 0.0;
    for (d, f) in df {
        data.push((cum_sum, f));
        cum_sum += d;
        data.push((cum_sum, f));
        max = f64::max(max, f);
    }
    let mut chart = builder.build_cartesian_2d(0f64..cum_sum, 0f64..max)?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(data.into_iter(), BLACK))?;
    Ok(())
}

fn df_coords(df: &DataFrame) -> Result<Vec<(f64, f64)>, Box<dyn std::error::Error>> {
    let domain = df
        .select_at_idx(0)
        .ok_or("query should return 2 columns")?
        .cast(&DataType::Float64)?;
    let range = df
        .select_at_idx(1)
        .ok_or("query should return 2 columns")?
        .cast(&DataType::Float64)?;
    domain
        .f64()?
        .into_iter()
        .zip(range.f64()?.into_iter())
        .map(|(o1, o2)| {
            Ok((
                o1.ok_or(format!(
                    "unsupported data type for line graph: {}",
                    df[0].dtype()
                ))?,
                o2.ok_or(format!(
                    "unsupported data type for line graph: {}",
                    df[1].dtype()
                ))?,
            ))
        })
        .collect()
}
