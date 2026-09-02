use std::io::{Cursor, Write};

use zip::{ZipWriter, write::SimpleFileOptions};

pub enum Cell {
    Text(String),
    Number(u64),
    Link { url: String, label: String },
}

impl From<String> for Cell {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for Cell {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

pub fn workbook(
    context: &[(String, String)],
    headers: &[&str],
    rows: &[Vec<Cell>],
) -> Result<Vec<u8>, zip::result::ZipError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (name, contents) in [
        ("[Content_Types].xml", CONTENT_TYPES.to_string()),
        ("_rels/.rels", ROOT_RELS.to_string()),
        ("xl/workbook.xml", WORKBOOK.to_string()),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.to_string()),
        ("xl/styles.xml", STYLES.to_string()),
        (
            "xl/worksheets/sheet1.xml",
            sheet_xml(
                &["Setting", "Value"],
                &context
                    .iter()
                    .map(|(key, value)| vec![key.clone().into(), value.clone().into()])
                    .collect::<Vec<_>>(),
            ),
        ),
        ("xl/worksheets/sheet2.xml", sheet_xml(headers, rows)),
    ] {
        zip.start_file(name, options)?;
        zip.write_all(contents.as_bytes())?;
    }
    Ok(zip.finish()?.into_inner())
}

fn sheet_xml(headers: &[&str], rows: &[Vec<Cell>]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews><sheetData><row r="1">"#,
    );
    for (column, header) in headers.iter().enumerate() {
        push_text(&mut xml, column, 1, header, true);
    }
    xml.push_str("</row>");
    for (index, row) in rows.iter().enumerate() {
        let row_number = index + 2;
        xml.push_str(&format!("<row r=\"{row_number}\">"));
        for (column, cell) in row.iter().enumerate() {
            match cell {
                Cell::Text(value) => push_text(&mut xml, column, row_number, value, false),
                Cell::Number(value) => xml.push_str(&format!(
                    "<c r=\"{}\"><v>{value}</v></c>",
                    reference(column, row_number)
                )),
                Cell::Link { url, label } => xml.push_str(&format!(
                    "<c r=\"{}\" s=\"2\"><f>HYPERLINK(&quot;{}&quot;,&quot;{}&quot;)</f></c>",
                    reference(column, row_number),
                    escape(&url.replace('"', "\"\"")),
                    escape(&label.replace('"', "\"\"")),
                )),
            }
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData><autoFilter ref=\"A1:");
    xml.push_str(&reference(headers.len().saturating_sub(1), rows.len() + 1));
    xml.push_str("\"/></worksheet>");
    xml
}

fn push_text(xml: &mut String, column: usize, row: usize, value: &str, header: bool) {
    let style = if header { " s=\"1\"" } else { "" };
    xml.push_str(&format!(
        "<c r=\"{}\" t=\"inlineStr\"{style}><is><t xml:space=\"preserve\">{}</t></is></c>",
        reference(column, row),
        escape(value)
    ));
}

fn reference(mut column: usize, row: usize) -> String {
    let mut letters = String::new();
    loop {
        letters.insert(0, (b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    format!("{letters}{row}")
}

fn escape(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            matches!(*character, '\u{9}' | '\u{a}' | '\u{d}') || *character >= '\u{20}'
        })
        .fold(String::new(), |mut result, character| {
            match character {
                '&' => result.push_str("&amp;"),
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '"' => result.push_str("&quot;"),
                '\'' => result.push_str("&apos;"),
                _ => result.push(character),
            }
            result
        })
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
const WORKBOOK: &str = r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Report Context" sheetId="1" r:id="rId1"/><sheet name="Results" sheetId="2" r:id="rId2"/></sheets></workbook>"#;
const WORKBOOK_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="3"><font/><font><b/></font><font><u/><color rgb="FF0563C1"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count="3"><xf/><xf fontId="1" applyFont="1"/><xf fontId="2" applyFont="1"/></cellXfs></styleSheet>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_xlsx_with_both_worksheets() {
        let bytes = workbook(
            &[("View".into(), "My Drive".into())],
            &["Name", "Link"],
            &[vec![
                "Report".into(),
                Cell::Link {
                    url: "https://drive.google.com/open?id=1&x=2".into(),
                    label: "Open".into(),
                },
            ]],
        )
        .expect("workbook");
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("valid archive");
        assert!(archive.by_name("xl/worksheets/sheet1.xml").is_ok());
        assert!(archive.by_name("xl/worksheets/sheet2.xml").is_ok());
    }
}
