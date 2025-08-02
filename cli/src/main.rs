use ipmpsc::SharedRingBuffer;
use zellix::EditorCommand;

fn main() {
    let session_id = std::env::var("KERBIN_SESSION").unwrap();
    let path = format!(
        "{}/kerbin/sessions/{}",
        dirs::data_dir().unwrap().display(),
        session_id
    );

    let command = std::env::args().skip(1).collect::<Vec<String>>().join(" ");
    let command = ron::from_str::<EditorCommand>(&command).unwrap();

    let sender = ipmpsc::Sender::new(SharedRingBuffer::open(&path).unwrap());

    sender.send(&command).unwrap();
}