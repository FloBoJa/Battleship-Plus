bs_plus_protocol = Proto("bs_plus", "Battleship Plus Protocol")
version_type = ProtoField.uint8("bs_plus.version", "Version", base.DEC)
length_type = ProtoField.uint16("bs_plus.length", "Payload Length", base.DEC)

bs_plus_protocol.fields = {version_type, length_type}

local protobuf_dissector = Dissector.get("protobuf")

function bs_plus_protocol.dissector(buffer, pinfo, tree)
    pinfo.cols.protocol = bs_plus_protocol.name;
    subtree = tree:add(bs_plus_protocol, buffer())
    version = buffer(0, 1):uint()
    length = buffer(1, 2):uint()
    subtree:add(version_type, buffer(0, 1))
    subtree:add(length_type, buffer(1, 2))
    pinfo.private["pb_msg_type"] = "message,battleshipplus.messages.PacketPayload"
    pcall(Dissector.call, protobuf_dissector, buffer(3, length):tvb(), pinfo, tree)
end

local udp_port = DissectorTable.get("udp.port")
udp_port:add(30303, bs_plus_protocol)
