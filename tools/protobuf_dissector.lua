bs_plus_protocol = Proto("bs_plus", "Battleship Plus Protocol")
version_type = ProtoField.uint8("bs_plus.version", "Version", base.DEC)
length_type = ProtoField.uint16("bs_plus.length", "Payload Length", base.DEC)

bs_plus_protocol.fields = {version_type, length_type}

local protobuf_dissector = Dissector.get("protobuf")
local quic_dissector = Dissector.get("quic")

local udp_data = Field.new("udp.payload")
local quic_stream_data = Field.new("quic.stream_data")

function bs_plus_protocol.dissector(buffer, pinfo, tree)
    if quic_stream_data() then
        buffer = quic_stream_data().value:tvb()
    elseif udp_data() then
        buffer = udp_data().value:tvb()
    else 
        return 0
    end
    
    total_length = 0
    while true do
        -- Minimum header size
        if buffer:len() < 3 then break end
        -- Assume version 1 of the protocol
        if buffer(0, 1):uint() ~= 1 then break end
        -- Minimum message length
        if buffer(1, 2):uint() + 3 > buffer:len() then break end
    
        pinfo.cols.protocol = bs_plus_protocol.name;
        subtree = tree:add(bs_plus_protocol, buffer())
        version = buffer(0, 1):uint()
        length = buffer(1, 2):uint()
        subtree:add(version_type, buffer(0, 1))
        subtree:add(length_type, buffer(1, 2))
        pinfo.private["pb_msg_type"] = "message,battleshipplus.messages.PacketPayload"
        pcall(Dissector.call, protobuf_dissector, buffer(3, length):tvb(), pinfo, subtree) 
        message_size = 3 + length
        total_length = total_length + message_size
        if message_size == buffer:len() then
            break
        end
        buffer = buffer(message_size, buffer:len() - message_size)
    end
    return total_length
end

register_postdissector(bs_plus_protocol)
