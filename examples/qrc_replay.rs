use qqmusic::qrc::parse_qrc;
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("需要 QRC 文件路径")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("QRC 超过样本限制".into());
    }
    let lyric = parse_qrc(std::str::from_utf8(&bytes)?)?;
    println!(
        "{}",
        serde_json::json!({"parsed":true,"lineCount":lyric.lines.len(),"wordCount":lyric.lines.iter().map(|line| line.words.len()).sum::<usize>()})
    );
    Ok(())
}
