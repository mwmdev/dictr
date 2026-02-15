{ pkgs ? import <nixpkgs> { config.allowUnfree = true; } }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Rust toolchain
    rustc
    cargo
    rustfmt
    clippy

    # whisper.cpp build deps (+ CUDA)
    cmake
    clang
    pkg-config
    cudaPackages.cuda_nvcc
    cudaPackages.cuda_cudart
    cudaPackages.cuda_cccl
    cudaPackages.libcublas

    # cpal (ALSA backend)
    alsa-lib

    # rdev (X11 backend)
    xorg.libX11
    xorg.libXi
    xorg.libXtst
    xorg.libXrandr

    # reqwest (TLS)
    openssl

    # Runtime: text injection
    xdotool
    xclip
  ];

  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  CMAKE_CUDA_ARCHITECTURES = "89";
  # Ensure the real NVIDIA driver is found before Nix CUDA stubs in the rpath
  CARGO_BUILD_RUSTFLAGS = "-C link-arg=-Wl,-rpath,/run/opengl-driver/lib";
  LD_LIBRARY_PATH = "/run/opengl-driver/lib:" + pkgs.lib.makeLibraryPath [
    pkgs.alsa-lib
    pkgs.xorg.libX11
    pkgs.xorg.libXi
    pkgs.xorg.libXtst
    pkgs.xorg.libXrandr
    pkgs.openssl
    pkgs.cudaPackages.cuda_cudart
    pkgs.cudaPackages.libcublas
  ];
}
