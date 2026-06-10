{
  description = "dictr push-to-talk voice dictation";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      cudaArches = [
        "50"
        "52"
        "53"
        "60"
        "61"
        "62"
        "70"
        "72"
        "75"
        "80"
        "86"
        "87"
        "89"
        "90"
        "100"
        "101"
        "103"
        "120"
        "121"
      ];
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            config.allowUnfree = true;
          };

          inherit (pkgs) lib;
          cudaPackages = pkgs.cudaPackages_12_9;

          runtimeTools = with pkgs; [
            ffmpeg
            procps
            pulseaudio
            xclip
            xdotool
          ];

          commonNativeBuildInputs = with pkgs; [
            clang
            cmake
            makeWrapper
            pkg-config
            rustPlatform.bindgenHook
          ];

          commonBuildInputs = with pkgs; [
            alsa-lib
            libx11
            libxi
            libxrandr
            libxtst
            openssl
          ];

          mkDictr = { cudaArch ? null }:
            let
              cuda = cudaArch != null;
            in
            pkgs.rustPlatform.buildRustPackage {
              pname = if cuda then "dictr-cuda-sm${cudaArch}" else "dictr";
              version = "0.2.1";
              src = ./.;

              cargoLock.lockFile = ./Cargo.lock;

              nativeBuildInputs = commonNativeBuildInputs
                ++ lib.optionals cuda [
                  cudaPackages.cuda_nvcc
                ];

              buildInputs = commonBuildInputs
                ++ lib.optionals cuda [
                  cudaPackages.cuda_cccl
                  cudaPackages.cuda_cudart
                  cudaPackages.libcublas
                ];

              buildFeatures = lib.optionals cuda [ "cuda" ];

              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

              CMAKE_CUDA_ARCHITECTURES = lib.optionalString cuda cudaArch;

              RUSTFLAGS = lib.optionalString cuda (
                lib.concatStringsSep " " [
                  "-C link-arg=-Wl,-rpath,/run/opengl-driver/lib"
                  "-C link-arg=-Wl,-rpath,${lib.getLib cudaPackages.cuda_cudart}/lib"
                  "-C link-arg=-Wl,-rpath,${lib.getLib cudaPackages.libcublas}/lib"
                  "-C link-arg=-L${lib.getLib cudaPackages.cuda_cudart}/lib/stubs"
                  "-Lnative=${lib.getLib cudaPackages.cuda_cudart}/lib/stubs"
                ]
              );

              postInstall = ''
                wrapProgram "$out/bin/dictr" \
                  --prefix PATH : ${lib.makeBinPath runtimeTools}
              '';

              doCheck = !cuda;

              meta.mainProgram = "dictr";
            };

          cudaPackagesByArch = lib.listToAttrs (map
            (cudaArch: {
              name = "dictr-cuda-sm${cudaArch}";
              value = mkDictr { inherit cudaArch; };
            })
            cudaArches);

          dictrCpu = mkDictr { };
        in
        {
          dictr-cpu = dictrCpu;
          default = dictrCpu;
        } // cudaPackagesByArch);

      apps = forAllSystems (system:
        let
          packages = self.packages.${system};
        in
        {
          default = {
            type = "app";
            program = "${packages.default}/bin/dictr";
          };
        });
    };
}
