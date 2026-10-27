#!/usr/bin/env python3
"""
Flash Layout and Boundary Verifier for Digivice ESP32-C6 (16MB Flash).
Verifies:
1. partitions.csv syntax, sector alignment (4KB), and non-overlapping layout.
2. Machine-checked boundary between factory app partition and storage partition (0x400000).
3. Storage constants in src/storage/mod.rs strictly align with partitions.csv.
4. Compiled ELF loadable segment bounds fit comfortably inside the factory app partition.
"""

import os
import re
import struct
import sys

TOTAL_FLASH_SIZE = 16 * 1024 * 1024  # 16 MB = 0x0100_0000
SECTOR_SIZE = 4096


def parse_partitions(csv_path):
    partitions = []
    with open(csv_path, "r", encoding="utf-8") as f:
        for line_num, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split(",")]
            if len(parts) < 5:
                print(f"ERROR: {csv_path}:{line_num} malformed partition entry: {line}", file=sys.stderr)
                sys.exit(1)
            name, ptype, subtype, offset_str, size_str = parts[:5]
            offset = int(offset_str, 0)
            size = int(size_str, 0)
            partitions.append({
                "name": name,
                "type": ptype,
                "subtype": subtype,
                "offset": offset,
                "size": size,
                "end": offset + size,
            })
    return partitions


def parse_storage_constants(storage_mod_path):
    with open(storage_mod_path, "r", encoding="utf-8") as f:
        content = f.read()

    def get_const(name):
        m = re.search(rf"pub const {name}:\s*u32\s*=\s*(0x[0-9a-fA-F_]+|\d+);", content)
        if not m:
            print(f"ERROR: Constant {name} not found in {storage_mod_path}", file=sys.stderr)
            sys.exit(1)
        return int(m.group(1).replace("_", ""), 0)

    return {
        "primary": get_const("CONFIG_FLASH_PRIMARY_OFFSET"),
        "secondary": get_const("CONFIG_FLASH_SECONDARY_OFFSET"),
        "blackbox": get_const("BLACKBOX_FLASH_OFFSET"),
        "snapshot_cap": get_const("BLACKBOX_SNAPSHOT_CAPACITY"),
    }


def parse_elf_loadable_size(elf_path):
    if not os.path.isfile(elf_path):
        return None
    with open(elf_path, "rb") as f:
        data = f.read()
    if data[:4] != b"\x7fELF" or data[4] != 1:
        return None
    e_phoff, = struct.unpack_from("<I", data, 0x1C)
    e_phentsize, e_phnum = struct.unpack_from("<HH", data, 0x2A)
    load_size = 0
    for i in range(e_phnum):
        offset = e_phoff + i * e_phentsize
        p_type, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_flags, p_align = struct.unpack_from(
            "<IIIIIIII", data, offset
        )
        if p_type == 1:  # PT_LOAD
            load_size += p_filesz
    return load_size


def parse_elf_memory_budget(elf_path):
    if not os.path.isfile(elf_path):
        return None
    with open(elf_path, "rb") as f:
        data = f.read()
    if data[:4] != b"\x7fELF" or data[4] != 1:
        return None
    e_shoff, = struct.unpack_from("<I", data, 0x20)
    e_shentsize, e_phnum, e_shstrndx = struct.unpack_from("<HHH", data, 0x2E)
    e_shnum, = struct.unpack_from("<H", data, 0x30)

    shstr_offset = e_shoff + e_shstrndx * e_shentsize
    _, _, _, _, str_offset, str_size, _, _, _, _ = struct.unpack_from("<IIIIIIIIII", data, shstr_offset)
    shstrtab = data[str_offset : str_offset + str_size]

    sram_allocated = 0
    stack_size = 0
    flash_cached = 0

    for i in range(e_shnum):
        sec = data[e_shoff + i * e_shentsize : e_shoff + (i + 1) * e_shentsize]
        name_idx, s_type, s_flags, s_addr, s_offset, s_size, _, _, _, _ = struct.unpack_from("<IIIIIIIIII", sec)
        name = shstrtab[name_idx:].split(b"\x00")[0].decode("latin1")
        if (s_flags & 0x2) and s_size > 0:
            if 0x40800000 <= s_addr < 0x40880000:
                if name == ".stack":
                    stack_size = s_size
                else:
                    sram_allocated += s_size
            elif 0x42000000 <= s_addr < 0x43000000:
                flash_cached += s_size

    return {
        "sram_static": sram_allocated,
        "stack_region": stack_size,
        "flash_cached": flash_cached,
    }


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    csv_path = os.path.join(root, "partitions.csv")
    storage_mod_path = os.path.join(root, "crates", "digivice-core", "src", "storage", "mod.rs")

    print(f"Checking Flash Layout Verification:")
    print(f"  Partitions CSV: {csv_path}")
    print(f"  Storage Constants: {storage_mod_path}")

    partitions = parse_partitions(csv_path)
    storage = parse_storage_constants(storage_mod_path)

    # 1. Check partition alignments and overlaps
    print("\n[1/4] Validating partition table alignment & overlaps...")
    prev_end = 0
    for p in sorted(partitions, key=lambda x: x["offset"]):
        if p["offset"] < prev_end:
            print(f"FATAL: Partition '{p['name']}' overlaps with previous partition at 0x{p['offset']:08X}", file=sys.stderr)
            sys.exit(1)
        if p["offset"] % SECTOR_SIZE != 0:
            print(f"FATAL: Partition '{p['name']}' offset 0x{p['offset']:08X} is not 4KB-aligned", file=sys.stderr)
            sys.exit(1)
        if p["size"] % SECTOR_SIZE != 0:
            print(f"FATAL: Partition '{p['name']}' size 0x{p['size']:08X} is not 4KB-aligned", file=sys.stderr)
            sys.exit(1)
        if p["end"] > TOTAL_FLASH_SIZE:
            print(f"FATAL: Partition '{p['name']}' ends at 0x{p['end']:08X}, exceeding 16MB limit (0x{TOTAL_FLASH_SIZE:08X})", file=sys.stderr)
            sys.exit(1)
        print(f"  - {p['name']:<10} 0x{p['offset']:08X}..0x{p['end']:08X} ({p['size'] // 1024} KB)")
        prev_end = p["end"]

    # 2. Check factory app vs storage boundary
    print("\n[2/4] Verifying factory app vs storage boundary...")
    factory = next((p for p in partitions if p["name"] == "factory"), None)
    storage_part = next((p for p in partitions if p["name"] == "storage"), None)
    if not factory:
        print("FATAL: 'factory' partition missing in partitions.csv", file=sys.stderr)
        sys.exit(1)
    if not storage_part:
        print("FATAL: 'storage' partition missing in partitions.csv", file=sys.stderr)
        sys.exit(1)

    if factory["end"] != storage_part["offset"]:
        print(f"FATAL: Boundary mismatch: factory ends at 0x{factory['end']:08X}, but storage begins at 0x{storage_part['offset']:08X}", file=sys.stderr)
        sys.exit(1)
    if storage_part["offset"] != 0x0040_0000:
        print(f"FATAL: Storage partition offset must be exactly 0x0040_0000 (4MB mark), got 0x{storage_part['offset']:08X}", file=sys.stderr)
        sys.exit(1)
    print(f"  OK: Machine-checked boundary confirmed at 0x{storage_part['offset']:08X} (4MB mark).")

    # 3. Check storage constants against storage partition
    print("\n[3/4] Checking firmware storage constants against partition bounds...")
    if storage["primary"] != storage_part["offset"]:
        print(f"FATAL: CONFIG_FLASH_PRIMARY_OFFSET (0x{storage['primary']:08X}) does not match storage partition start (0x{storage_part['offset']:08X})", file=sys.stderr)
        sys.exit(1)
    if storage["secondary"] != storage["primary"] + SECTOR_SIZE:
        print(f"FATAL: CONFIG_FLASH_SECONDARY_OFFSET must be primary + 4KB", file=sys.stderr)
        sys.exit(1)
    if storage["blackbox"] < storage["secondary"] + SECTOR_SIZE:
        print(f"FATAL: BLACKBOX_FLASH_OFFSET overlaps with config sectors", file=sys.stderr)
        sys.exit(1)
    blackbox_end = storage["blackbox"] + storage["snapshot_cap"] * 64
    if blackbox_end > storage_part["end"]:
        print(f"FATAL: Blackbox capacity ends at 0x{blackbox_end:08X}, exceeding storage partition 0x{storage_part['end']:08X}", file=sys.stderr)
        sys.exit(1)
    print(f"  OK: Sector A = 0x{storage['primary']:08X}, Sector B = 0x{storage['secondary']:08X}")
    print(f"  OK: Blackbox = 0x{storage['blackbox']:08X}..0x{blackbox_end:08X} ({storage['snapshot_cap']} snapshots, {blackbox_end - storage['blackbox']} bytes)")

    # 4. Check firmware binary sizes if release build exists
    print("\n[4/5] Checking release firmware binary fit within factory partition...")
    digivice_elf = os.path.join(root, "target", "riscv32imac-unknown-none-elf", "release", "digivice")
    selftest_elf = os.path.join(root, "target", "riscv32imac-unknown-none-elf", "release", "selftest")

    for elf_path, name in [(digivice_elf, "digivice"), (selftest_elf, "selftest")]:
        load_size = parse_elf_loadable_size(elf_path)
        if load_size is not None:
            utilization = (load_size / factory["size"]) * 100
            if load_size > factory["size"]:
                print(f"FATAL: Release binary '{name}' ({load_size} bytes) exceeds factory partition size ({factory['size']} bytes)!", file=sys.stderr)
                sys.exit(1)
            print(f"  - {name:<10}: {load_size:>7} bytes ({load_size / 1024:>6.1f} KB) / {factory['size'] // 1024} KB ({utilization:4.1f}% utilization)")
        else:
            print(f"  - {name:<10}: (binary not found, skipped ELF loadable check)")

    # 5. Check SRAM allocation and stack headroom
    print("\n[5/5] Checking SRAM allocation, heap budget, and stack headroom...")
    TOTAL_HP_SRAM = 512 * 1024  # 512 KB HP SRAM
    HEAP_SIZE = 128 * 1024       # 128 KB heap configured in digivice.rs
    MIN_STACK_HEADROOM = 64 * 1024  # At least 64 KB guaranteed stack headroom

    for elf_path, name in [(digivice_elf, "digivice"), (selftest_elf, "selftest")]:
        budget = parse_elf_memory_budget(elf_path)
        if budget is not None:
            static_sram = budget["sram_static"]
            stack_region = budget["stack_region"]
            free_headroom = stack_region - HEAP_SIZE if name == "digivice" else stack_region - (32 * 1024)

            print(f"  - {name:<10}:")
            print(f"      Static SRAM  : {static_sram:>7} bytes ({static_sram / 1024:>5.1f} KB)")
            print(f"      Stack Region : {stack_region:>7} bytes ({stack_region / 1024:>5.1f} KB)")
            print(f"      Heap Resrv   : {HEAP_SIZE if name == 'digivice' else 32 * 1024:>7} bytes ({(HEAP_SIZE if name == 'digivice' else 32 * 1024) / 1024:>5.1f} KB)")
            print(f"      Stack Headrm : {free_headroom:>7} bytes ({free_headroom / 1024:>5.1f} KB)")

            if static_sram > 320 * 1024:
                print(f"FATAL: Static SRAM for '{name}' ({static_sram} B) exceeds 320 KB limit!", file=sys.stderr)
                sys.exit(1)
            if free_headroom < MIN_STACK_HEADROOM:
                print(f"FATAL: Stack headroom for '{name}' ({free_headroom} B) is below {MIN_STACK_HEADROOM} B!", file=sys.stderr)
                sys.exit(1)
        else:
            print(f"  - {name:<10}: (binary not found, skipped SRAM budget check)")

    print("\nPASS: All Flash layout partition boundaries, sector alignments, SRAM budgets, and binary limits verified successfully.\n")


if __name__ == "__main__":
    main()
