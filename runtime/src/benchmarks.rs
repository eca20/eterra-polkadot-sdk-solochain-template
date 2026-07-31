// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

frame_benchmarking::define_benchmarks!(
    [frame_benchmarking, BaselineBench::<Runtime>]
    [frame_system, SystemBench::<Runtime>]
    [pallet_balances, Balances]
    [pallet_timestamp, Timestamp]
    [pallet_sudo, Sudo]
    [pallet_utility, Utility]
    [pallet_eterra, Eterra]
    [pallet_eterra_daily_slots, EterraDailySlots]
    [pallet_eterra_simple_matchmaker, EterraSimpleMatchMaker]
    [pallet_eterra_faucet, EterraFaucet]
    [pallet_eterra_gamer, EterraGamer]
    [pallet_eterra_randomness, EterraRandomness]
    [pallet_eterra_creatures, EterraCreatures]
    [pallet_eterra_magic, EterraMagic]
    [pallet_eterra_game_results, EterraGameResults]
    [pallet_eterra_monte_carlo_ai, EterraMonteCarloAi]
    [pallet_eterra_tcg, EterraTCG]
    [pallet_eterra_game_authority, EterraGameAuthority]
    [pallet_eterra_media, EterraMedia]
    [pallet_eterra_authority, EterraAuthority]
    [pallet_eterra_economy, EterraEconomy]
    [pallet_eterra_profile, EterraProfile]
    [pallet_eterra_flow, EterraFlow]
);
