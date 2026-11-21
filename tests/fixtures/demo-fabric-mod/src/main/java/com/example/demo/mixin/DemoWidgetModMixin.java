package com.example.demo.mixin;

import com.example.demo.DemoWidgetMod;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(DemoWidgetMod.class)
public class DemoWidgetModMixin {
    @Inject(method = "greet", at = @At("HEAD"), remap = false)
    private void ummp$onGreet(CallbackInfoReturnable<String> cir) {
        System.out.println("[demowidget] mixin applied");
    }
}
