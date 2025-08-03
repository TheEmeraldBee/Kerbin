def main [] {
  # Open yazi at the current path, and print the selected files to stdout.
  let paths = yazi --chooser-file=/dev/stdout

  # Split the files by rows.
  let command = ($paths | each {|line| $line | split row "\n"})

  # Check if no files were selected, and exit if none are.
  if ($command | get 0 | str trim | is-empty) {
    exit 0
  }

  $command | each {|p| ./target/release/kerbin-cli ("OpenFile\(" ++ $"\"($p)\"" ++ ")") }
}

