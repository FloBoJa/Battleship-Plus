bs_plus_protocol = Proto("bs_plus", "Battleship Plus Protocol")
version_type = ProtoField.uint8("bs_plus.version", "Version", base.DEC)
op_code_type = ProtoField.uint8("bs_plus.op_code", "OpCode", base.HEX)
length_type = ProtoField.uint16("bs_plus.length", "Payload Length", base.DEC)
data_type = ProtoField.bytes("bs_plus.data", "Data", base.SPACE)

bs_plus_protocol.fields = {version_type, op_code_type, length_type, data_type}

local protobuf_dissector = Dissector.get("protobuf")

function bs_plus_protocol.dissector(buffer, pinfo, tree)
    pinfo.cols.protocol = bs_plus_protocol.name;
    subtree = tree:add(bs_plus_protocol, buffer())
    version = buffer(0, 1):uint()
    op_code = buffer(1, 1):uint()
    length = buffer(2, 2):uint()
    subtree:add(version_type, buffer(0, 1))
    subtree:add(op_code_type, buffer(1, 1))
    subtree:add(length_type, buffer(2, 2))
    if op_code == 0x00 then
        pinfo.private["pb_msg_type"] = "message,battleshipplus.messages.ServerAdvertisement"
        pcall(Dissector.call, protobuf_dissector, buffer(4, buffer:len() - 4):tvb(), pinfo, tree)
    else
        subtree:add(data_type, buffer(4, buffer:len() - 4))
    end
end

local udp_port = DissectorTable.get("udp.port")
udp_port:add(30303, bs_plus_protocol)
