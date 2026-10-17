#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QrSizeError {
    TooSmall,
    InvalidCode,
}

/// Encodes the WhatsApp pairing payload as terminal cells with a four-module quiet zone.
/// The payload itself is never returned or formatted as text.
pub fn encode_cells(payload: &str, width: u16, height: u16) -> Result<Vec<String>, QrSizeError> {
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|_| QrSizeError::InvalidCode)?;
    let side = code.width();
    let modules = side + 8;
    if u32::from(width) < (modules * 2) as u32 || u32::from(height) < modules as u32 {
        return Err(QrSizeError::TooSmall);
    }
    let colors = code.to_colors();
    let mut rows = Vec::with_capacity(modules);
    for y in 0..modules {
        let mut row = String::with_capacity(modules * 2);
        for x in 0..modules {
            let dark = x >= 4
                && y >= 4
                && x < side + 4
                && y < side + 4
                && colors[(y - 4) * side + x - 4] == qrcode::Color::Dark;
            row.push_str(if dark { "██" } else { "  " });
        }
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{QrSizeError, encode_cells};

    #[test]
    fn qr_has_quiet_zone() {
        let rows = encode_cells("fixture payload", 100, 100).unwrap();
        assert!(rows.first().unwrap().chars().all(|cell| cell == ' '));
        assert!(rows.last().unwrap().chars().all(|cell| cell == ' '));
        assert!(
            rows.iter()
                .all(|row| row.starts_with("        ") && row.ends_with("        "))
        );
        assert!(rows.iter().any(|row| row.contains("██")));
    }

    #[test]
    fn qr_reports_a_small_terminal() {
        assert_eq!(encode_cells("fixture", 10, 10), Err(QrSizeError::TooSmall));
    }
}
