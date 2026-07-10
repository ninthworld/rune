//! RUNE CLI client. Prints valid actions as a numbered list, reads a choice from
//! stdin, sends `ChooseAction`. Also the harness an LLM agent drives (dev sequence
//! steps 3-4 in docs/brief.md).

fn main() {
    let sample = rune_protocol::ValidAction {
        id: "a1".into(),
        label: "Pass".into(),
        subject: vec![],
    };
    println!("rune-cli scaffold — sample action: [{}] {}", sample.id, sample.label);
}
