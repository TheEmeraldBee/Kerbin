def main [session] {
  let file = rg --files --ignore | fzf

  if not ($file | is-empty) {
    cargo run -p kerbin -- command -s $session -c $"o ($file)"
  }
}
