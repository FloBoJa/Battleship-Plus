#!/bin/bash

tcpdump --interface=any \
        -n \
        -C 256 \
        -w /data/quic-30305-dump-"$(date +%F_%H-%M)".pcap \
        -Z root \
        "udp port 30305" \
        &

cd /data || exit

SSLKEYLOGFILE="/data/tls-$(date +%F_%H-%M).keylog" \
RUST_LOG=debug \
    unbuffer /battleship_plus/server 2>&1 \
      | tee \
      >(split -d -b 268435456 - "battleship_plus-$(date +%F_%H-%M).log")