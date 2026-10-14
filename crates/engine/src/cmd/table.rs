//! Table Design and Table Layout tabs.

use deckcraft_model::{ShapeKind, Table};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("table.insertRowAbove", "Insert Above", ["Table Layout", "Rows & Columns"], None, "{row?, id?}", has_selection, |s, p| rows_cols(
            s, p, "rowAbove"
        )),
        cmd!("table.insertRowBelow", "Insert Below", ["Table Layout", "Rows & Columns"], None, "{row?, id?}", has_selection, |s, p| rows_cols(
            s, p, "rowBelow"
        )),
        cmd!("table.insertColumnLeft", "Insert Left", ["Table Layout", "Rows & Columns"], None, "{col?, id?}", has_selection, |s, p| rows_cols(
            s, p, "colLeft"
        )),
        cmd!("table.insertColumnRight", "Insert Right", ["Table Layout", "Rows & Columns"], None, "{col?, id?}", has_selection, |s, p| rows_cols(
            s, p, "colRight"
        )),
        cmd!("table.deleteRow", "Delete Rows", ["Table Layout", "Rows & Columns"], None, "{row?, id?}", has_selection, |s, p| rows_cols(
            s, p, "delRow"
        )),
        cmd!("table.deleteColumn", "Delete Columns", ["Table Layout", "Rows & Columns"], None, "{col?, id?}", has_selection, |s, p| rows_cols(
            s, p, "delCol"
        )),
        cmd!("table.merge", "Merge Cells", ["Table Layout", "Merge"], None, "{from: [r, c], to: [r, c], id?}", has_selection, merge),
        cmd!("table.split", "Split Cells", ["Table Layout", "Merge"], None, "{cell: [r, c], id?}", has_selection, split),
        cmd!(
            "table.style",
            "Table Styles",
            ["Table Design", "Table Styles"],
            None,
            "{style: medium2-accent1|light1-accent2|dark1-tx1|grid|none…, id?}",
            has_selection,
            style
        ),
        cmd!(
            "table.options",
            "Table Style Options",
            ["Table Design", "Table Style Options"],
            None,
            "{headerRow?, totalRow?, bandedRows?, firstColumn?, lastColumn?, bandedColumns?: bool, id?}",
            has_selection,
            options
        ),
        cmd!(
            "table.cellFill",
            "Shading",
            ["Table Design", "Table Styles"],
            None,
            "{color? | none?, cells?: [[r,c],…] (default: selected cells or all), id?}",
            has_selection,
            cell_fill
        ),
        cmd!(
            "table.borders",
            "Borders",
            ["Table Design", "Draw Borders"],
            None,
            "{which: all|outside|inside|insideH|insideV|top|bottom|left|right|diagDown|diagUp|none, color?, width?: pt, dash?: solid|dash|dot…, cells?: [[r,c],…] (default: selected cells or all), id?}",
            has_selection,
            borders
        ),
        cmd!(
            "table.cellMargins",
            "Cell Margins",
            ["Table Layout", "Alignment"],
            None,
            "{preset?: normal|none|narrow|wide, left?, right?, top?, bottom?: pt, cells?, id?}",
            has_selection,
            cell_margins
        ),
        cmd!(
            "table.cellAlign",
            "Align Cell Text",
            ["Table Layout", "Alignment"],
            None,
            "{anchor: top|middle|bottom, cells?, id?}",
            has_selection,
            cell_align
        ),
        cmd!("table.distributeRows", "Distribute Rows", ["Table Layout", "Cell Size"], None, "{id?}", has_selection, |s, p| distribute(s, p, true)),
        cmd!("table.distributeColumns", "Distribute Columns", ["Table Layout", "Cell Size"], None, "{id?}", has_selection, |s, p| distribute(
            s, p, false
        )),
        cmd!("table.columnWidth", "Width", ["Table Layout", "Cell Size"], None, "{col, width: pt, id?}", has_selection, column_width),
        cmd!("table.rowHeight", "Height", ["Table Layout", "Cell Size"], None, "{row, height: pt, id?}", has_selection, row_height),
        cmd!(noundo "table.selectCells", "Select Cells", [], None, "{from: [r,c], to: [r,c], id?}", has_selection, select_cells),
    ]
}

fn cell_param(p: &Value, key: &str) -> Option<(usize, usize)> {
    let a = p.get(key)?.as_array()?;
    Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize))
}

/// The table's row/col to act on: params, the edited cell, or the selected cells.
fn at(s: &Session, p: &Value) -> (usize, usize) {
    let st = s.active();
    let cell = st.and_then(|d| d.selection.text.as_ref().and_then(|t| t.cell)).or_else(|| st.and_then(|d| d.selection.cells.map(|c| c.1)));
    (usize_param(p, "row").or(cell.map(|c| c.0)).unwrap_or(0), usize_param(p, "col").or(cell.map(|c| c.1)).unwrap_or(0))
}

fn with_table(s: &mut Session, p: &Value, cmd: &str, f: impl Fn(&mut Table) -> Result<()>) -> Result<Value> {
    let id = match id_param(p, "id", cmd)? {
        Some(i) => i,
        None => {
            let st = s.doc()?;
            st.selection
                .text
                .as_ref()
                .filter(|t| t.cell.is_some())
                .map(|t| t.shape)
                .or(st.selection.cells.map(|c| c.0))
                .or_else(|| st.selection.shapes.first().copied())
                .ok_or_else(|| bad(cmd, "select a table"))?
        }
    };
    let r = edit_shapes(s, &json!({"id": id}), cmd, |sh| match &mut sh.kind {
        ShapeKind::Table(t) => {
            f(t)?;
            t.normalize();
            let (w, h) = (t.width(), t.height());
            if let Some(x) = sh.xfrm.as_mut() {
                x.w = w;
                x.h = h;
            }
            Ok(())
        }
        _ => Err(bad(cmd, "not a table")),
    })?;
    // The edited cell may have moved out of range.
    s.select(|doc, sel| {
        let dims = super::current_shapes(doc, sel)
            .and_then(|v| deckcraft_model::find_shape(v, id))
            .and_then(|x| if let ShapeKind::Table(t) = &x.kind { Some((t.n_rows(), t.n_cols())) } else { None });
        if let (Some(t), Some((nr, nc))) = (sel.text.as_mut(), dims)
            && let Some((r, c)) = t.cell
            && (r >= nr || c >= nc)
        {
            t.cell = Some((r.min(nr.saturating_sub(1)), c.min(nc.saturating_sub(1))));
            t.anchor = (0, 0);
            t.caret = (0, 0);
        }
    })?;
    Ok(r)
}

fn rows_cols(s: &mut Session, p: &Value, op: &str) -> Result<Value> {
    let (r, c) = at(s, p);
    let op = op.to_string();
    with_table(s, p, "table", move |t| {
        match op.as_str() {
            "rowAbove" => t.insert_row(r),
            "rowBelow" => t.insert_row(r + 1),
            "colLeft" => t.insert_col(c),
            "colRight" => t.insert_col(c + 1),
            "delRow" => {
                if !t.delete_row(r) {
                    return Err(bad("table.deleteRow", "can't delete the last row"));
                }
            }
            _ => {
                if !t.delete_col(c) {
                    return Err(bad("table.deleteColumn", "can't delete the last column"));
                }
            }
        }
        Ok(())
    })
}

fn merge(s: &mut Session, p: &Value) -> Result<Value> {
    let sel = s.doc()?.selection.cells;
    let from = cell_param(p, "from").or(sel.map(|c| c.1)).ok_or_else(|| bad("table.merge", "missing `from`"))?;
    let to = cell_param(p, "to").or(sel.map(|c| c.2)).ok_or_else(|| bad("table.merge", "missing `to`"))?;
    with_table(
        s,
        p,
        "table.merge",
        move |t| if t.merge(from.0, from.1, to.0, to.1) { Ok(()) } else { Err(bad("table.merge", "can't merge those cells")) },
    )
}

fn split(s: &mut Session, p: &Value) -> Result<Value> {
    let cell = cell_param(p, "cell").unwrap_or_else(|| at(s, p));
    with_table(s, p, "table.split", move |t| if t.split(cell.0, cell.1) { Ok(()) } else { Err(bad("table.split", "that cell isn't merged")) })
}

fn style(s: &mut Session, p: &Value) -> Result<Value> {
    let st = str_param(p, "style").ok_or_else(|| bad("table.style", "missing `style`"))?.to_string();
    with_table(s, p, "table.style", move |t| {
        t.style = st.clone();
        Ok(())
    })
}

fn options(s: &mut Session, p: &Value) -> Result<Value> {
    with_table(s, p, "table.options", |t| {
        if let Some(v) = bool_param(p, "headerRow") {
            t.first_row = v;
        }
        if let Some(v) = bool_param(p, "totalRow") {
            t.last_row = v;
        }
        if let Some(v) = bool_param(p, "bandedRows") {
            t.band_row = v;
        }
        if let Some(v) = bool_param(p, "firstColumn") {
            t.first_col = v;
        }
        if let Some(v) = bool_param(p, "lastColumn") {
            t.last_col = v;
        }
        if let Some(v) = bool_param(p, "bandedColumns") {
            t.band_col = v;
        }
        Ok(())
    })
}

/// The cells a command acts on: `cells`, else the selected cell range, else the cell holding the
/// caret, else every cell.
fn target_cells(s: &Session, p: &Value) -> Result<impl Fn(&Table) -> Vec<(usize, usize)> + use<>> {
    let cells: Option<Vec<(usize, usize)>> = p
        .get("cells")
        .and_then(Value::as_array)
        .map(|a| a.iter().take(100_000).filter_map(|c| Some((c.get(0)?.as_u64()? as usize, c.get(1)?.as_u64()? as usize))).collect());
    let st = s.doc()?;
    let sel = st.selection.cells.map(|(_, a, b)| (a, b)).or_else(|| st.selection.text.as_ref().and_then(|t| t.cell).map(|c| (c, c)));
    Ok(move |t: &Table| match (&cells, sel) {
        (Some(c), _) => c.clone(),
        (None, Some((a, b))) => {
            // The selection can name cells outside the table; only walk the ones that exist.
            let (r1, c1) = (a.0.max(b.0).min(t.n_rows().saturating_sub(1)), a.1.max(b.1).min(t.n_cols().saturating_sub(1)));
            (a.0.min(b.0)..=r1).flat_map(|r| (a.1.min(b.1)..=c1).map(move |c| (r, c))).collect()
        }
        _ => (0..t.n_rows()).flat_map(|r| (0..t.n_cols()).map(move |c| (r, c))).collect(),
    })
}

fn cell_fill(s: &mut Session, p: &Value) -> Result<Value> {
    let fill = if bool_or(p, "none", false) { Some(deckcraft_model::Fill::None) } else { color_param(p, "color").map(deckcraft_model::Fill::solid) };
    let targets = target_cells(s, p)?;
    with_table(s, p, "table.cellFill", move |t| {
        for (r, c) in targets(t) {
            if let Some(cell) = t.cell_mut(r, c) {
                cell.fill.clone_from(&fill);
            }
        }
        Ok(())
    })
}

const LEFT: usize = 0;
const RIGHT: usize = 1;
const TOP: usize = 2;
const BOTTOM: usize = 3;

fn borders(s: &mut Session, p: &Value) -> Result<Value> {
    use deckcraft_model::Line;
    let which = str_param(p, "which").unwrap_or("all").to_string();
    const WHICH: [&str; 12] = ["all", "outside", "inside", "insideH", "insideV", "top", "bottom", "left", "right", "diagDown", "diagUp", "none"];
    if !WHICH.contains(&which.as_str()) {
        return Err(bad("table.borders", format!("unknown `which` {which:?}; one of {}", WHICH.join(", "))));
    }
    let color = color_param(p, "color").unwrap_or_else(|| deckcraft_model::ColorRef::rgb(deckcraft_color::Rgba::rgb(0, 0, 0)));
    let width = f64_param(p, "width").filter(|w| w.is_finite()).unwrap_or(1.0).clamp(0.0, 100.0);
    let mut line = Line::solid(color, width);
    if let Some(d) = str_param(p, "dash").filter(|d| *d != "solid") {
        if !deckcraft_model::style::Dash::PRESETS.contains(&d) {
            return Err(bad("table.borders", format!("unknown dash {d:?}; one of {}", deckcraft_model::style::Dash::PRESETS.join(", "))));
        }
        line.dash = Some(deckcraft_model::style::Dash::Preset { name: d.to_string() });
    }
    // "none" clears to no line, so the table style's own lines don't show through either.
    let line = if which == "none" { Line::none() } else { line };
    let targets = target_cells(s, p)?;
    with_table(s, p, "table.borders", move |t| {
        let list = targets(t);
        let (Some(r0), Some(r1), Some(c0), Some(c1)) =
            (list.iter().map(|x| x.0).min(), list.iter().map(|x| x.0).max(), list.iter().map(|x| x.1).min(), list.iter().map(|x| x.1).max())
        else {
            return Ok(());
        };
        let (rows, cols) = (t.n_rows(), t.n_cols());
        // Set one side of a cell and the facing side of its neighbour, as PowerPoint does.
        let set = |t: &mut Table, r: usize, c: usize, side: usize| {
            if let Some(cell) = t.cell_mut(r, c) {
                cell.borders[side] = Some(line.clone());
            }
            let n = match side {
                LEFT => c.checked_sub(1).map(|c| (r, c, RIGHT)),
                RIGHT => (c + 1 < cols).then_some((r, c + 1, LEFT)),
                TOP => r.checked_sub(1).map(|r| (r, c, BOTTOM)),
                _ => (r + 1 < rows).then_some((r + 1, c, TOP)),
            };
            if let Some((r, c, side)) = n
                && let Some(cell) = t.cell_mut(r, c)
            {
                cell.borders[side] = Some(line.clone());
            }
        };
        for (r, c) in list {
            if r < r0 || r > r1 || c < c0 || c > c1 {
                continue;
            }
            let outer = [c == c0, c == c1, r == r0, r == r1];
            for side in [LEFT, RIGHT, TOP, BOTTOM] {
                let horizontal = side == TOP || side == BOTTOM;
                let on = match which.as_str() {
                    "all" | "none" => true,
                    "outside" => outer[side],
                    "inside" => !outer[side],
                    "insideH" => horizontal && !outer[side],
                    "insideV" => !horizontal && !outer[side],
                    "top" => side == TOP && outer[side],
                    "bottom" => side == BOTTOM && outer[side],
                    "left" => side == LEFT && outer[side],
                    "right" => side == RIGHT && outer[side],
                    _ => false,
                };
                if on {
                    set(t, r, c, side);
                }
            }
            if let Some(cell) = t.cell_mut(r, c) {
                match which.as_str() {
                    "diagDown" => cell.diag_down = Some(line.clone()),
                    "diagUp" => cell.diag_up = Some(line.clone()),
                    "none" => {
                        cell.diag_down = None;
                        cell.diag_up = None;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })
}

fn cell_margins(s: &mut Session, p: &Value) -> Result<Value> {
    // PowerPoint's presets (left/right, top/bottom).
    let base = match str_param(p, "preset") {
        Some("none") => [0.0; 4],
        Some("narrow") => [3.6; 4],
        Some("wide") => [14.4, 14.4, 14.4, 14.4],
        Some("normal") | None => [7.2, 7.2, 3.6, 3.6],
        Some(o) => return Err(bad("table.cellMargins", format!("unknown preset {o:?}; one of normal, none, narrow, wide"))),
    };
    let side = |k: &str, i: usize| f64_param(p, k).filter(|v| v.is_finite()).map_or(base[i], |v| v.clamp(0.0, 400.0));
    let m = [side("left", 0), side("right", 1), side("top", 2), side("bottom", 3)];
    let targets = target_cells(s, p)?;
    with_table(s, p, "table.cellMargins", move |t| {
        for (r, c) in targets(t) {
            if let Some(cell) = t.cell_mut(r, c) {
                cell.margins = Some(m);
            }
        }
        Ok(())
    })
}

fn cell_align(s: &mut Session, p: &Value) -> Result<Value> {
    use deckcraft_model::text::Anchor;
    let a = match str_param(p, "anchor").unwrap_or("top") {
        "middle" | "center" | "ctr" => Anchor::Middle,
        "bottom" | "b" => Anchor::Bottom,
        "top" | "t" => Anchor::Top,
        o => return Err(bad("table.cellAlign", format!("unknown anchor {o:?}; one of top, middle, bottom"))),
    };
    let targets = target_cells(s, p)?;
    with_table(s, p, "table.cellAlign", move |t| {
        for (r, c) in targets(t) {
            if let Some(cell) = t.cell_mut(r, c) {
                cell.anchor = Some(a);
            }
        }
        Ok(())
    })
}

fn distribute(s: &mut Session, p: &Value, rows: bool) -> Result<Value> {
    with_table(s, p, "table.distribute", move |t| {
        if rows {
            let h = t.height() / t.n_rows().max(1) as f64;
            t.rows.iter_mut().for_each(|r| r.height = h);
        } else {
            let w = t.width() / t.n_cols().max(1) as f64;
            t.cols.iter_mut().for_each(|c| *c = w);
        }
        Ok(())
    })
}

fn column_width(s: &mut Session, p: &Value) -> Result<Value> {
    let (_, c) = at(s, p);
    let w = f64_param(p, "width").ok_or_else(|| bad("table.columnWidth", "missing `width`"))?.clamp(4.0, 4000.0);
    with_table(s, p, "table.columnWidth", move |t| {
        if let Some(x) = t.cols.get_mut(c) {
            *x = w;
        }
        Ok(())
    })
}

fn row_height(s: &mut Session, p: &Value) -> Result<Value> {
    let (r, _) = at(s, p);
    let h = f64_param(p, "height").ok_or_else(|| bad("table.rowHeight", "missing `height`"))?.clamp(4.0, 4000.0);
    with_table(s, p, "table.rowHeight", move |t| {
        if let Some(x) = t.rows.get_mut(r) {
            x.height = h;
        }
        Ok(())
    })
}

fn select_cells(s: &mut Session, p: &Value) -> Result<Value> {
    let id = match id_param(p, "id", "table.selectCells")? {
        Some(i) => i,
        None => s.doc()?.selection.shapes.first().copied().ok_or_else(|| bad("table.selectCells", "select a table"))?,
    };
    let from = cell_param(p, "from").unwrap_or((0, 0));
    let to = cell_param(p, "to").unwrap_or(from);
    s.select(|_, sel| {
        sel.cells = Some((id, from, to));
        sel.shapes = vec![id];
        sel.text = None;
    })?;
    ok()
}
