def main [session: string, current_buffer] {
  mut current_path = $current_buffer | str join
  if $current_path == "<scratch>" {
    $current_path = "./"
  }

  let paths = yazi --chooser-file=/dev/stdout $current_path

  let lines = ($paths | split row "\n")

  $lines | each {
    |line|
    print $line
    cargo run -p kerbin -- command -s $session -c $"o ($line)"
  }
}
