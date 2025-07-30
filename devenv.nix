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
  ];
  languages.rust = {
    enable = true;
    channel = "stable";
  };
}
