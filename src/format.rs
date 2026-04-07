/// Split text into chunks at line boundaries, each <= limit chars.
pub fn split_message(text: &str, limit: usize) -> Vec<String> {
    if text.len() <= limit {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.split('\n') {
        // +1 for the newline
        if !current.is_empty() && current.len() + line.len() + 1 > limit {
            chunks.push(current);
            current = String::new();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        // If a single line exceeds limit, hard-split it
        if line.len() > limit {
            let mut remaining = line;
            while !remaining.is_empty() {
                let end = remaining.floor_char_boundary(limit.min(remaining.len()));
                let end = if end == 0 { remaining.len().min(limit.max(1)) } else { end };
                if !current.is_empty() {
                    chunks.push(current);
                }
                current = remaining[..end].to_string();
                remaining = &remaining[end..];
            }
        } else {
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}
