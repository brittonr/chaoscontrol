# Assemble a guest initrd that preserves Aspen's absolute signed-output identity.
#
# The caller supplies the exact signed output selected by `sourceTrust`, the matching
# collaboration-native driver binary, and ChaosControl's musl guest wrapper. Every path in
# the three closures is copied to the same absolute path inside the initrd so Aspen's source
# admission re-measures the output it was signed for.
{
  pkgs,
  guestWrapper,
  aspenTestBinary,
  aspenOutput,
  sourceTrust,
  name ? "chaoscontrol-initrd-molten",
}:
let
  closure = pkgs.closureInfo {
    rootPaths = [
      guestWrapper
      aspenTestBinary
      aspenOutput
    ];
  };
in
pkgs.runCommand name
  {
    nativeBuildInputs = [ pkgs.cpio ];
  }
  ''
    mkdir -p root/{dev,proc,sys,sys/kernel/debug,etc/molten,data,usr/bin,tmp}

    while IFS= read -r path; do
      [ -n "$path" ] || continue
      mkdir -p "root$path"
      cp -a "$path/." "root$path/"
    done < ${closure}/store-paths

    cp ${guestWrapper}/bin/chaoscontrol-molten-guest root/init
    cp ${aspenTestBinary} root/usr/bin/molten-collaborationnative
    cp ${sourceTrust} root/etc/molten/source-gate-trust.json
    chmod +x root/init root/usr/bin/molten-collaborationnative

    (cd root && find . -print0 | cpio --null -o -H newc --quiet) | gzip -9 > $out
  ''
