//! # Spout Output (Windows)
//!
//! GPU texture sharing output via Spout2 using the `windows` crate.
//!
//! ## Architecture
//!
//! There is no maintained Rust Spout2 *sender* crate, so we implement the
//! Spout sender protocol directly:
//!
//! 1. Create a standalone D3D11 device (wgpu on Windows uses D3D12, so we
//!    maintain a separate D3D11 device solely for Spout sharing).
//! 2. Create a D3D11 shared texture with `D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`.
//! 3. Register the sender in the two Spout2 shared-memory mappings:
//!    `SpoutSenderNames` (flat name array) + per-sender `SharedTextureInfo`.
//! 4. Each frame: receive pre-read CPU bytes from the async readback pool,
//!    then `UpdateSubresource` into the D3D11 shared texture under the keyed mutex.

#![cfg(target_os = "windows")]

use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE, INVALID_HANDLE_VALUE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_CREATE_DEVICE_FLAG, D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{IDXGIKeyedMutex, IDXGIResource};
use windows::Win32::System::Memory::{
    CreateFileMappingA, FILE_MAP_ALL_ACCESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
};

/// DXGI_FORMAT_B8G8R8A8_UNORM numeric value for Spout2 protocol.
const DXGI_FORMAT_B8G8R8A8_UNORM_VALUE: u32 = 87;

const SPOUT_MAX_NAME_LEN: usize = 256;
const SPOUT_MAX_SENDERS: usize = 64;

#[repr(C)]
struct SharedTextureInfo {
    share_handle: u32,
    width: u32,
    height: u32,
    format: u32,
    usage: u32,
    description: [u8; 256],
    partner_id: u32,
}

/// Spout sender — shares the wgpu render target with other apps on this machine.
pub struct SpoutOutput {
    sender_name: String,
    d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    shared_texture: Option<ID3D11Texture2D>,
    share_handle: HANDLE,
    width: u32,
    height: u32,
    _sender_names_map: HANDLE,
    _sender_info_map: HANDLE,
}

// D3D11 objects are used from a single thread; mark Send/Sync so the type
// satisfies the InterAppVideo trait bounds.
unsafe impl Send for SpoutOutput {}
unsafe impl Sync for SpoutOutput {}

impl SpoutOutput {
    pub fn new(name: &str) -> anyhow::Result<Self> {
        unsafe {
            let mut d3d_device = None;
            let mut d3d_context = None;

            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                Some(&mut d3d_context),
            )?;

            let d3d_device = d3d_device
                .ok_or_else(|| anyhow::anyhow!("[Spout] D3D11CreateDevice returned no device"))?;
            let d3d_context = d3d_context.ok_or_else(|| {
                anyhow::anyhow!("[Spout] D3D11CreateDevice returned no context")
            })?;

            log::info!("[Spout] D3D11 device created for sender '{}'", name);

            let mut output = Self {
                sender_name: name.to_string(),
                d3d_device,
                d3d_context,
                shared_texture: None,
                share_handle: HANDLE::default(),
                width: 0,
                height: 0,
                _sender_names_map: HANDLE::default(),
                _sender_info_map: HANDLE::default(),
            };

            output.create_shared_texture(64, 64)?;
            log::info!("[Spout] Sender '{}' registered early (placeholder 64x64)", name);

            Ok(output)
        }
    }

    pub fn submit_bytes(
        &mut self,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> anyhow::Result<()> {
        if self.shared_texture.is_none() || self.width != width || self.height != height {
            self.shared_texture = None;
            self.create_shared_texture(width, height)?;
        }

        unsafe {
            let d3d_tex = self.shared_texture.as_ref().unwrap();
            let keyed_mutex: IDXGIKeyedMutex = d3d_tex.cast()?;
            keyed_mutex.AcquireSync(0, 0xFFFFFFFF)?;

            let row_pitch = width * 4;
            self.d3d_context.UpdateSubresource(
                d3d_tex,
                0,
                None,
                bytes.as_ptr() as *const _,
                row_pitch,
                0,
            );

            keyed_mutex.ReleaseSync(0)?;
        }

        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.sender_name
    }

    fn create_shared_texture(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        unsafe {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32,
            };

            let mut tex = None;
            self.d3d_device.CreateTexture2D(&desc, None, Some(&mut tex))?;
            let tex: ID3D11Texture2D =
                tex.ok_or_else(|| anyhow::anyhow!("[Spout] CreateTexture2D returned None"))?;

            let dxgi_resource: IDXGIResource = tex.cast()?;
            let handle = dxgi_resource.GetSharedHandle()?;

            match tex.cast::<IDXGIKeyedMutex>() {
                Ok(keyed_mutex) => {
                    keyed_mutex.AcquireSync(0, 0xFFFFFFFF)?;
                    keyed_mutex.ReleaseSync(0)?;
                }
                Err(e) => {
                    log::warn!("[Spout] Failed to cast to keyed mutex: {:?}", e);
                }
            }

            log::info!("[Spout] Shared texture {}x{} created, handle={:?}", width, height, handle);

            if !self._sender_info_map.is_invalid() && !self._sender_info_map.0.is_null() {
                CloseHandle(self._sender_info_map).ok();
            }

            self.share_handle = handle;
            self.shared_texture = Some(tex);
            self.width = width;
            self.height = height;

            let (names_map, info_map) = self.register_spout_sender(width, height, handle)?;
            self._sender_names_map = names_map;
            self._sender_info_map = info_map;
        }
        Ok(())
    }

    unsafe fn register_spout_sender(
        &self,
        width: u32,
        height: u32,
        handle: HANDLE,
    ) -> anyhow::Result<(HANDLE, HANDLE)> {
        let map_name = windows::core::s!("SpoutSenderNames");
        let map_size = (SPOUT_MAX_SENDERS * SPOUT_MAX_NAME_LEN) as u32;
        let hmap = CreateFileMappingA(
            INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, map_size, map_name,
        )?;

        let view = MapViewOfFile(hmap, FILE_MAP_ALL_ACCESS, 0, 0, 0);
        if view.Value.is_null() {
            CloseHandle(hmap).ok();
            return Err(anyhow::anyhow!("[Spout] MapViewOfFile failed for SpoutSenderNames"));
        }

        {
            let base = view.Value as *mut u8;
            let name_bytes = self.sender_name.as_bytes();
            let mut already_present = false;

            for i in 0..SPOUT_MAX_SENDERS {
                let slot = base.add(i * SPOUT_MAX_NAME_LEN);
                if *slot == 0 { break; }
                let mut len = 0usize;
                while len < SPOUT_MAX_NAME_LEN {
                    if *slot.add(len) == 0 { break; }
                    len += 1;
                }
                if len == name_bytes.len()
                    && std::slice::from_raw_parts(slot, len) == name_bytes
                {
                    already_present = true;
                    break;
                }
            }

            if !already_present {
                for i in 0..SPOUT_MAX_SENDERS {
                    let slot = base.add(i * SPOUT_MAX_NAME_LEN);
                    if *slot == 0 {
                        let copy_len = name_bytes.len().min(SPOUT_MAX_NAME_LEN - 1);
                        std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), slot, copy_len);
                        *slot.add(copy_len) = 0;
                        if i + 1 < SPOUT_MAX_SENDERS {
                            *base.add((i + 1) * SPOUT_MAX_NAME_LEN) = 0;
                        }
                        log::info!("[Spout] Registered '{}' in SpoutSenderNames (slot {})", self.sender_name, i);
                        break;
                    }
                }
            }
        }

        UnmapViewOfFile(view).ok();

        let sender_cstr = std::ffi::CString::new(self.sender_name.as_str())
            .map_err(|e| anyhow::anyhow!("[Spout] invalid sender name: {}", e))?;

        let hmap2 = CreateFileMappingA(
            INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0,
            std::mem::size_of::<SharedTextureInfo>() as u32,
            windows::core::PCSTR(sender_cstr.as_ptr() as *const u8),
        )?;

        let view2 = MapViewOfFile(hmap2, FILE_MAP_ALL_ACCESS, 0, 0, 0);
        if view2.Value.is_null() {
            CloseHandle(hmap2).ok();
            return Err(anyhow::anyhow!("[Spout] MapViewOfFile failed for sender info '{}'", self.sender_name));
        }

        let handle_u32 = handle.0 as i32 as u32;

        let mut description = [0u8; 256];
        if let Ok(exe_path) = std::env::current_exe() {
            let path_str = exe_path.to_string_lossy();
            let path_bytes = path_str.as_bytes();
            let copy_len = path_bytes.len().min(255);
            description[..copy_len].copy_from_slice(&path_bytes[..copy_len]);
        }

        let info_ptr = view2.Value as *mut SharedTextureInfo;
        *info_ptr = SharedTextureInfo {
            share_handle: handle_u32,
            width,
            height,
            format: DXGI_FORMAT_B8G8R8A8_UNORM_VALUE,
            usage: 0,
            description,
            partner_id: 0,
        };

        UnmapViewOfFile(view2).ok();

        log::info!(
            "[Spout] Sender info written for '{}' {}x{} (handle=0x{:08x})",
            self.sender_name, width, height, handle_u32,
        );
        Ok((hmap, hmap2))
    }

    unsafe fn unregister_spout_sender(&self) {
        let map_name = windows::core::s!("SpoutSenderNames");
        let Ok(hmap) = CreateFileMappingA(
            INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0,
            (SPOUT_MAX_SENDERS * SPOUT_MAX_NAME_LEN) as u32, map_name,
        ) else { return; };

        let view = MapViewOfFile(hmap, FILE_MAP_ALL_ACCESS, 0, 0, 0);
        if !view.Value.is_null() {
            let base = view.Value as *mut u8;
            let name_bytes = self.sender_name.as_bytes();

            let mut found_idx: Option<usize> = None;
            let mut total_count = 0usize;
            for i in 0..SPOUT_MAX_SENDERS {
                let slot = base.add(i * SPOUT_MAX_NAME_LEN);
                if *slot == 0 { total_count = i; break; }
                let mut len = 0usize;
                while len < SPOUT_MAX_NAME_LEN {
                    if *slot.add(len) == 0 { break; }
                    len += 1;
                }
                if found_idx.is_none()
                    && len == name_bytes.len()
                    && std::slice::from_raw_parts(slot, len) == name_bytes
                {
                    found_idx = Some(i);
                }
                if i == SPOUT_MAX_SENDERS - 1 { total_count = SPOUT_MAX_SENDERS; }
            }

            if let Some(idx) = found_idx {
                let remaining = total_count.saturating_sub(idx + 1);
                if remaining > 0 {
                    std::ptr::copy(
                        base.add((idx + 1) * SPOUT_MAX_NAME_LEN),
                        base.add(idx * SPOUT_MAX_NAME_LEN),
                        remaining * SPOUT_MAX_NAME_LEN,
                    );
                }
                let last = if total_count > 0 { total_count - 1 } else { 0 };
                std::ptr::write_bytes(base.add(last * SPOUT_MAX_NAME_LEN), 0, SPOUT_MAX_NAME_LEN);
                log::info!("[Spout] Unregistered '{}' from SpoutSenderNames", self.sender_name);
            }

            UnmapViewOfFile(view).ok();
        }
        CloseHandle(hmap).ok();
    }
}

impl super::InterAppVideo for SpoutOutput {
    fn publish_frame(&mut self, _texture: &wgpu::Texture, _device: &wgpu::Device, _queue: &wgpu::Queue) {
        // SpoutOutput is driven via OutputManager + submit_bytes() using the
        // async readback pool. This trait method is not used on Windows.
        log::trace!("[Spout] publish_frame called on '{}' (noop)", self.sender_name);
    }

    fn receive_frame(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue) -> Option<wgpu::Texture> {
        None
    }

    fn name(&self) -> &str {
        &self.sender_name
    }

    fn is_available() -> bool {
        true
    }
}

impl Drop for SpoutOutput {
    fn drop(&mut self) {
        unsafe {
            self.unregister_spout_sender();
            if !self._sender_info_map.is_invalid() && !self._sender_info_map.0.is_null() {
                CloseHandle(self._sender_info_map).ok();
            }
            if !self._sender_names_map.is_invalid() && !self._sender_names_map.0.is_null() {
                CloseHandle(self._sender_names_map).ok();
            }
        }
        log::info!("[Spout] Sender '{}' dropped", self.sender_name);
    }
}

/// Spout input receiver for receiving video from other apps
pub struct SpoutInput {
    sender_name: String,
}

impl SpoutInput {
    pub fn new(sender_name: &str) -> Self {
        Self { sender_name: sender_name.to_string() }
    }

    pub fn try_receive(&mut self, _device: &wgpu::Device) -> Option<wgpu::Texture> {
        None
    }
}
