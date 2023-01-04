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
    /battleship_plus/server
