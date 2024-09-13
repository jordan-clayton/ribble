#[cfg(all(target_os = "linux", feature = "hipblas"))]
use std::path::Path;

#[cfg(feature = "cuda")]
use nvml_wrapper::{cuda_driver_version_major, cuda_driver_version_minor};

#[cfg(feature = "cuda")]
use crate::constants;

#[cfg(all(target_os = "windows", feature = "cuda"))]
pub fn check_gpu_target() -> bool {
    use nvml_wrapper::Nvml;
    let nvml = Nvml::init();
    if let Err(_) = &nvml {
        return false;
    };

    let nvml = nvml.unwrap();
    let device_count = nvml.device_count();
    if let Err(_) = &device_count {
        return false;
    };

    let device_count = device_count.unwrap();
    if device_count < 1 {
        return false;
    };

    let cuda_version = nvml.sys_cuda_driver_version();
    if let Err(_) = &cuda_version {
        return false;
    }

    let cuda_version = cuda_version.unwrap();
    let sys_major_version = cuda_driver_version_major(cuda_version);
    let sys_minor_version = cuda_driver_version_minor(cuda_version);
    sys_major_version == constants::CUDA_MAJOR
}

#[cfg(all(target_os = "linux", feature = "cuda"))]
pub fn check_gpu_target() -> bool {
    use nvml_wrapper::Nvml;
    let nvml = Nvml::init();
    if let Err(_) = &nvml {
        return false;
    };

    let nvml = nvml.unwrap();
    let device_count = nvml.device_count();
    if let Err(_) = &device_count {
        return false;
    };

    let device_count = device_count.unwrap();
    if device_count < 1 {
        return false;
    };

    let cuda_version = nvml.sys_cuda_driver_version();
    if let Err(_) = &cuda_version {
        return false;
    }

    let cuda_version = cuda_version.unwrap();
    let sys_major_version = cuda_driver_version_major(cuda_version);
    sys_major_version == constants::CUDA_MAJOR
}

#[cfg(all(target_os = "linux", feature = "hipblas"))]
pub fn check_gpu_target() -> bool {
    let hip = env::var("HIP_PATH").is_ok();
    let common_paths = [
        "/opt/rocm/lib/libhipblas.so",
        "/opt/rocm/hip/lib/libhipblas.so",
        "opt/rocm/lib/librocblas.so",
        "opt/rocm/hipblas",
    ];
    let found_path = common_paths.iter().any(|&path| Path::new(path).exists());
    let blas = env::var("HIP_BLAS_PATH").is_ok();

    (hip && blas) || found_path
}

#[cfg(all(feature = "metal", target_arch = "x86_64"))]
pub fn check_gpu_target() -> bool {
    use metal;
    let available_devices = metal::Device::all();

    available_devices
        .iter()
        .any(|d| d.name().to_lowercase().contains("amd"))
}

// Apple Silicon is fully supported by Whisper.cpp
#[cfg(all(feature = "metal", target_arch = "aarch64"))]
pub fn check_gpu_target() -> bool {
    true
}

#[cfg(not(feature = "_gpu"))]
pub fn check_gpu_target() -> bool {
    false
}
