use kerbin_core::*;

pub async fn open_default_buffer(bufs: ResMut<Buffers>) {
    get!(mut bufs);

    let mut buffer = TextBuffer::scratch();

    buffer.action(Insert {
        byte: 0,
        content: include_str!("tutor.txt").to_string(),
    });

    buffer.path = "<tutor>".to_string();

    bufs.new(buffer).await;
}
