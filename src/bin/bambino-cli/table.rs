#![cfg(feature = "std")]

use std::io::{self, Write};

pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: Vec<&str>) -> Self {
        Self {
            headers: headers.into_iter().map(String::from).collect(),
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, cells: Vec<&str>) {
        self.rows
            .push(cells.into_iter().map(String::from).collect());
    }

    pub fn print(&self) {
        self.write_to(&mut io::stdout());
    }

    pub fn write_to(&self, w: &mut impl Write) {
        let col_count = self.headers.len();
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        let separator_width: usize = widths.iter().sum::<usize>() + (col_count - 1) * 3;

        write_row(w, &self.headers, &widths);
        let _ = writeln!(w, "{:─<width$}", "", width = separator_width);
        for row in &self.rows {
            write_row(w, row, &widths);
        }
    }
}

fn write_row(w: &mut impl Write, cells: &[String], widths: &[usize]) {
    let formatted: Vec<String> = cells
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let w = widths.get(i).copied().unwrap_or(0);
            format!("{:<width$}", cell, width = w)
        })
        .collect();
    let _ = writeln!(w, "{}", formatted.join(" │ "));
}
