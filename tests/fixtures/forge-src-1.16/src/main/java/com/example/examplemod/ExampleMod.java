package com.example.examplemod;

import net.minecraft.block.AbstractBlock;
import net.minecraft.block.Block;
import net.minecraft.block.material.Material;
import net.minecraft.item.BlockItem;
import net.minecraft.item.Item;
import net.minecraftforge.fml.common.Mod;

// Registers the example Block and its BlockItem. AbstractBlock stays in comments.
@Mod("examplemod")
public class ExampleMod {
    public static final String MODID = "examplemod";

    public static final Block EXAMPLE_BLOCK =
            new Block(AbstractBlock.Properties.of(Material.STONE));

    public static final Item EXAMPLE_ITEM =
            new BlockItem(EXAMPLE_BLOCK, new Item.Properties());

    public String describe() {
        return "AbstractBlock was renamed in 1.17";
    }
}
