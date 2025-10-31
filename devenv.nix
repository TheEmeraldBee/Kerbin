{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:
{
  packages = with pkgs; [
    bacon
    cargo-flamegraph
  ];
  languages.rust = {
    enable = true;
    channel = "stable";
  };
}
