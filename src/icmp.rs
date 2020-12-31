use packet::builder::Builder;
use packet::icmp;
use packet::Error;
use packet::Packet;
use rand::random;

const TOKEN_SIZE: usize = 8;

pub fn make_echo_request(
    ident: u16,
    seq_cnt: u16,
    size: usize,
) -> Result<(Vec<u8>, [u8; TOKEN_SIZE]), Error> {
    let token: [u8; TOKEN_SIZE] = random();
    let mut payload = vec![0; size];
    {
        let (left, _) = payload.split_at_mut(TOKEN_SIZE);
        left.copy_from_slice(&token);
    }

    let echo_request = icmp::Builder::default()
        .echo()?
        .request()?
        .identifier(ident)?
        .sequence(seq_cnt)?
        .payload(&payload)?
        .build()?;
    Ok((echo_request, token))
}

pub fn parse_token(buf: &[u8]) -> Result<(u16, [u8; 8]), Error> {
    let packet = icmp::Packet::new(buf)?;
    let echo_reply = packet.echo()?;
    if !echo_reply.is_reply() {
        return Err(Error::InvalidPacket);
    }
    if echo_reply.payload().as_ref().len() < 8 {
        return Err(Error::InvalidPacket);
    }
    let mut token = [0; 8];
    token.copy_from_slice(&echo_reply.payload()[..8]);
    Ok((echo_reply.sequence(), token))
}
