def main [session] {
  let file = rg --files --ignore | fzf

  if not ($file | is-empty) {
    cargo run -- command -s $session -c $"o ($file)"
  }
}
