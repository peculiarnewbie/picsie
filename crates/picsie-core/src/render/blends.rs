//! SeparableBlend.swift / LayerAppearance.swift, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Skia runtime blenders replace the source Core Image filters.
//! Compute in the canvas's sRGB space and composite premultiplied alpha only after blending.
use crate::model::Blend;
use anyhow::{Result, anyhow};
use skia_safe::{self as sk, Paint};
use std::{cell::RefCell, collections::HashMap};
thread_local! { static CACHE: RefCell<HashMap<Blend, sk::Blender>> = RefCell::new(HashMap::new()); }
pub(super) fn apply(paint: &mut Paint, mode: Blend) -> Result<()> {
    let expression = match mode {
        Blend::LinearBurn => "max(0.0, b+s-1.0)",
        Blend::LinearDodge => "min(1.0, b+s)",
        Blend::VividLight => "s < 0.5 ? burn(b, 2.0*s) : dodge(b, 2.0*s-1.0)",
        Blend::LinearLight => "clamp(b+2.0*s-1.0, 0.0, 1.0)",
        Blend::PinLight => "s < 0.5 ? min(b, 2.0*s) : max(b, 2.0*s-1.0)",
        Blend::HardMix => "(s < 0.5 ? burn(b, 2.0*s) : dodge(b, 2.0*s-1.0)) < 0.5 ? 0.0 : 1.0",
        Blend::Subtract => "max(0.0, b-s)",
        Blend::Divide => "s == 0.0 ? 1.0 : min(1.0, b/s)",
        // Source routes these through Core Image too, to preserve transparent brush edges.
        Blend::ColorBurn => "burn(b,s)",
        Blend::ColorDodge => "dodge(b,s)",
        _ => return Ok(()),
    };
    CACHE.with(|cache| {
        let mut cache=cache.borrow_mut();
        if !cache.contains_key(&mode) {
            let source=format!(r#"
                float burn(float b, float s) {{ return b == 1.0 ? 1.0 : s == 0.0 ? 0.0 : 1.0-min(1.0,(1.0-b)/s); }}
                float dodge(float b, float s) {{ return b == 0.0 ? 0.0 : s == 1.0 ? 1.0 : min(1.0,b/(1.0-s)); }}
                float channel(float b, float s) {{ return {expression}; }}
                half4 main(half4 src, half4 dst) {{
                    float sa=src.a, da=dst.a;
                    float3 s=sa > 0.0 ? float3(src.rgb)/sa : float3(0.0);
                    float3 b=da > 0.0 ? float3(dst.rgb)/da : float3(0.0);
                    float3 mixed=float3(channel(b.r,s.r),channel(b.g,s.g),channel(b.b,s.b));
                    return half4(src.rgb*(1.0-da)+dst.rgb*(1.0-sa)+mixed*sa*da, sa+da-sa*da);
                }}
            "#);
            let effect=sk::RuntimeEffect::make_for_blender(source, None).map_err(|e| anyhow!("Cannot compile blend mode: {e}"))?;
            let blender=effect.make_blender(sk::Data::new_empty(), None).ok_or_else(||anyhow!("Cannot create blend mode"))?;
            cache.insert(mode, blender);
        }
        paint.set_blender(cache.get(&mode).cloned());
        Ok(())
    })
}
